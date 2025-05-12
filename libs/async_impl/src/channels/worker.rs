use std::{collections::BinaryHeap, time::Duration};

use anyhow::Context;
use mempool::Transaction;
use tokio::{select, sync, task::JoinHandle, time::Instant};

use crate::{Mempool, channels::drain_strategy::DrainStrategy};

use super::drain_strategy::DrainRequest;

#[derive(Clone)]
pub struct Queue {
    channels: Channels,

    /// Handle to the worker task that manages the internal storage of the queue.
    /// Abort this task to drop the associated memory and stop
    runner_handle: std::sync::Arc<JoinHandle<Option<()>>>,
}

#[async_trait::async_trait]
impl Mempool for Queue {
    async fn submit(&self, tx: Transaction) -> anyhow::Result<()> {
        self.channels
            .submittance_source
            .send(tx)
            .await
            .context("could not submit transaction to queue")
    }
    async fn drain(&self, n: usize, timeout_us: u64) -> anyhow::Result<Vec<Transaction>> {
        let (req, rx_drainage) = DrainRequest::new_with_timeout(n, timeout_us);
        self.channels
            .drain_request_source
            .send(req)
            .await
            .context("could not send drain request to queue")?;
        rx_drainage
            .await
            .context("could not receive drainage result from queue")
    }
}
pub struct Cfg {
    /// Initial capacity of the queue. It will grow as needed as items are added.
    /// # Note
    /// At the moment the maximum size of the queue is not capped.
    pub capacity: usize,
    /// Number of [`Transaction`]s to keep in the submitter channels buffer before
    /// blocking senders.
    pub submittance_back_pressure: usize,
}

#[derive(Debug, Clone)]
pub struct Channels {
    submittance_source: sync::mpsc::Sender<Transaction>,
    drain_request_source: sync::mpsc::Sender<DrainRequest>,
}

impl Queue {
    const DRAIN_RETRY_DELAY: Duration = Duration::from_nanos(100);

    pub fn start(cfg: Cfg) -> Self {
        let (channels, internal_channels) = prepare_channels(&cfg);

        let runner_handle =
            std::sync::Arc::new(tokio::task::spawn(Self::run(cfg, internal_channels)));
        Self {
            runner_handle,
            channels,
        }
    }

    async fn run(cfg: Cfg, mut channels: InternalChannels) -> Option<()> {
        let mut storage = BinaryHeap::with_capacity(cfg.capacity);

        loop {
            select! {
                t = channels.submittance_sink.recv() => {
                    storage.push(t?);
                }
                req = channels.drain_request_sink.recv() => {
                    let req = req?;
                    match req.wait_strategy {
                        DrainStrategy::DrainMax => Self::handle_drain_max(req, &mut storage),
                        DrainStrategy::WaitForN(_) => {
                            Self::handle_drain_waiting(req, &mut storage, &mut channels.drain_request_source).await;
                        }
                    }
                }
            }
        }
    }

    fn handle_drain_max(req: DrainRequest, storage: &mut BinaryHeap<Transaction>) {
        let mut drained = Vec::with_capacity(req.n);
        for _ in 0..req.n {
            let Some(item) = storage.pop() else {
                break;
            };
            drained.push(item);
        }

        // TODO: Feed back drained elements in case of error
        req.send_back.send(drained).inspect_err(|_|eprintln!("Warn! Queue has been drained but requester has hung up. Drained elements are thrown away.")).ok();
    }

    async fn handle_drain_waiting(
        req: DrainRequest,
        storage: &mut BinaryHeap<Transaction>,
        drain_request_source: &mut sync::mpsc::Sender<DrainRequest>,
    ) {
        let timeout = match req.wait_strategy {
            DrainStrategy::DrainMax => return,
            DrainStrategy::WaitForN(timeout) => timeout,
        };

        // stop waiting if there are enough elements in the queue or the timeout is reached
        if (storage.len() >= req.n) || (Instant::now() + Self::DRAIN_RETRY_DELAY > timeout) {
            Self::handle_drain_max(req, storage);
            return;
        }
        // if there are not enough elements in the buffer, wait a little bit before issuing another drain request
        tokio::time::sleep(Self::DRAIN_RETRY_DELAY).await;
        drain_request_source
            .send(req)
            .await
            .inspect_err(|_| {
                eprintln!("Warn! Could not send drain request as channels are closed.")
            })
            .ok();
    }

    /// Stops the manager task of the queue and drops all included items
    pub fn stop(self) {
        // TODO: We might collect all remaining items in the queue and return them here.
        self.runner_handle.abort();
    }
}

struct InternalChannels {
    submittance_sink: sync::mpsc::Receiver<Transaction>,
    drain_request_sink: sync::mpsc::Receiver<DrainRequest>,
    drain_request_source: sync::mpsc::Sender<DrainRequest>,
}

fn prepare_channels(cfg: &Cfg) -> (Channels, InternalChannels) {
    let (submittance_source, submittance_sink) = sync::mpsc::channel(cfg.submittance_back_pressure);
    let (drain_request_source, drain_request_sink) = sync::mpsc::channel(10);

    (
        Channels {
            submittance_source,
            drain_request_source: drain_request_source.clone(),
        },
        InternalChannels {
            submittance_sink,
            drain_request_sink,
            drain_request_source,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::time;

    use super::*;
    use mempool::Transaction;

    fn setup_queue() -> Queue {
        // Small back pressure buffer
        let cfg = Cfg {
            capacity: 10,
            submittance_back_pressure: 10,
        };
        Queue::start(cfg)
    }

    #[tokio::test]
    async fn test_submit_and_drain_max() {
        let queue = setup_queue();

        let tx1 = Transaction::with_empty_load("tx1", 100, 1);
        let tx2 = Transaction::with_empty_load("tx2", 200, 2);
        let tx3 = Transaction::with_empty_load("tx3", 100, 0);

        queue.submit(tx1.clone()).await.unwrap();
        queue.submit(tx2.clone()).await.unwrap();
        queue.submit(tx3.clone()).await.unwrap();

        tokio::time::sleep(Duration::from_millis(1)).await;
        let result = queue.drain(2, 0).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], tx2);
        assert_eq!(result[1], tx3);

        queue.stop();
    }

    #[tokio::test]
    async fn test_drain_waiting_timeout_returns_partial_or_empty() {
        let queue = setup_queue();

        // No submissions
        // Use a small timeout to force immediate return
        let start = time::Instant::now();
        let drained = queue.drain(1, 1).await.unwrap();
        let elapsed = start.elapsed();
        // Should return quickly without items
        assert!(elapsed < Duration::from_millis(100));
        assert!(drained.is_empty());

        queue.stop();
    }

    #[tokio::test]
    async fn test_drain_waiting_succeeds_when_items_arrive() {
        let queue = setup_queue();

        // Spawn a delayed submission
        let delayed_queue = queue.clone();
        tokio::spawn(async move {
            time::sleep(Duration::from_millis(50)).await;
            delayed_queue
                .submit(Transaction::with_empty_load("tx_delayed", 150, 5))
                .await
                .unwrap();
        });

        // Drain waiting for 1 transaction, with timeout_us large enough
        let drained = queue.drain(1, 200_000).await.unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "tx_delayed");

        queue.stop();
    }
}
