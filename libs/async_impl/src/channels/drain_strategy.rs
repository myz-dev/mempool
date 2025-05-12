use std::time::Duration;

use mempool::Transaction;
use tokio::{sync, time::Instant};

pub type SendBack = sync::oneshot::Sender<Vec<Transaction>>;
pub type ReceiveDrainage = sync::oneshot::Receiver<Vec<Transaction>>;

/// Strategy to employ when draining items.
#[derive(Debug, Clone, Copy)]
pub enum DrainStrategy {
    /// Try to drain `n` items. If there are less than `n` items in the queue at the time the drain
    /// operation starts, these items are returned immediately, without a wait for more items.
    DrainMax(usize),
    /// Try to drain `n` items from the queue.
    /// If the internal timer reaches the specified [`Instant`], the drain strategy will be converted
    /// into `DrainMax` (e.g. at most `n` items will be returned).
    WaitForN { n: usize, timeout: Instant },
}

#[derive(Debug)]
pub struct DrainRequest {
    pub n: usize,
    pub wait_strategy: DrainStrategy,
    pub send_back: SendBack,
}

impl DrainStrategy {
    /// Creates a new [`DrainStrategy`] with at most `n` items to drain.
    pub fn new_standard(n: usize) -> Self {
        Self::DrainMax(n)
    }

    /// Creates a new [`DrainStrategy`] that tries to drain `n` elements for `timeout_us`.
    /// Should the timeout be exceeded without the number of items in the queue reaching `n`,
    /// all available items are drained.
    pub fn new_timeout(n: usize, timeout_us: u64) -> Self {
        Self::WaitForN {
            n,
            timeout: Instant::now() + Duration::from_micros(timeout_us),
        }
    }
}

impl DrainRequest {
    pub fn new_with_timeout(n: usize, timeout_us: u64) -> (Self, ReceiveDrainage) {
        let (send_back, rx) = sync::oneshot::channel();
        (
            Self {
                n,
                wait_strategy: DrainStrategy::new_timeout(n, timeout_us),
                send_back,
            },
            rx,
        )
    }
}
