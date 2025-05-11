use std::{
    collections::BinaryHeap,
    fmt::Debug,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{anyhow, bail};
use crossbeam::channel::{Receiver, Sender, TryRecvError};
use mempool::{Mempool, Transaction};

struct StorageFactory;

impl StorageFactory {
    /// Creates a new [`Storage`] instance with given `capacity` that is ready to submit and drain
    /// items from its queue.
    fn new_queue<T: Debug + Ord + Send + 'static>(capacity: usize) -> Channels<T> {
        Storage::start(capacity)
    }
}

/// The [`Ord`] implementation of parameter `T` needs to be in line with its desired
/// priority ordering.
///
/// [`std::cmp::Ordering::Greater`] corresponds to a higher priority, [`std::cmp::Ordering::Less`] to a lower one.
#[derive(Debug)]
struct Storage<T: Debug + Ord> {
    max_heap: BinaryHeap<T>,

    submitter_sink: Receiver<T>,

    drain_source: Sender<Vec<T>>,
    drain_command_sink: Receiver<usize>,

    running: Arc<AtomicBool>,
}

#[derive(Debug)]
struct Channels<T: Debug + Ord> {
    item_source: Sender<T>,

    drain_sink: Receiver<Vec<T>>,
    drain_command_source: Sender<usize>,

    queue_running: Arc<AtomicBool>,
}

impl<T: Debug + Ord + Send + 'static> Storage<T> {
    fn start(capacity: usize) -> Channels<T> {
        let (tx, rx) = crossbeam::channel::unbounded();
        let (tx_drain, rx_drain) = crossbeam::channel::bounded(1);
        let (tx_command, rx_command) = crossbeam::channel::bounded(1);
        let running = Arc::new(AtomicBool::new(true));
        let queue_running = Arc::clone(&running);

        let storage = Self {
            max_heap: BinaryHeap::with_capacity(capacity),
            submitter_sink: rx,
            drain_source: tx_drain,
            drain_command_sink: rx_command,
            running,
        };

        let wait_for_runner = Arc::new((Mutex::new(false), Condvar::new()));
        let spun_up_notifier = Arc::clone(&wait_for_runner);

        std::thread::spawn(move || {
            if let Err(e) = storage.run(spun_up_notifier) {
                eprintln!("Error! Queue has shut down: {e}");
            }
        });
        // Wait for the mempool runner to start up.
        let (lock, cvar) = &*wait_for_runner;
        let mut started = lock
            .lock()
            .expect("Runner thread does not panic while holding the lock.");
        while !*started {
            started = cvar.wait(started).unwrap();
        }

        Channels {
            item_source: tx,
            drain_sink: rx_drain,
            drain_command_source: tx_command,
            queue_running,
        }
    }

    /// This functions blocks the thread it is running on.
    /// It tries to receive messages on the submittance and drainage channels, as long as the channels are open.
    fn run(mut self, cond_var: Arc<(Mutex<bool>, Condvar)>) -> anyhow::Result<()> {
        Self::notify_about_start(cond_var)?;

        while self.running.load(Ordering::Relaxed) {
            self.submit_or_continue()?;
            self.drain_or_continue()?;

            // crossbeam::select! {
            //     recv(self.drain_command_sink) -> msg => println!("DRAIN COMMAND!"),

            //  }
        }

        Ok(())
    }

    /// Uses the conditional variable `cond_var` to notify the main thread that the runner has started.
    fn notify_about_start(cond_var: Arc<(Mutex<bool>, Condvar)>) -> anyhow::Result<()> {
        let mut started = cond_var
            .0
            .lock()
            .map_err(|_| anyhow!("Unexpected lock contention on startup of memory pool!"))?;
        *started = true;
        cond_var.1.notify_all();
        Ok(())
    }

    /// Receives a message and adds it to the queue when there is a new message in the channel.
    /// # Error
    /// Returns an error if the submittance channel is disconnected.
    fn submit_or_continue(&mut self) -> anyhow::Result<()> {
        match self.submitter_sink.try_recv() {
            Ok(t) => self.max_heap.push(t),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => bail!("Submittance channel is disconnected"),
        }
        Ok(())
    }

    fn drain_or_continue(&mut self) -> anyhow::Result<()> {
        let count = match self.drain_command_sink.try_recv() {
            Ok(n) => n,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => bail!("Drain command channel is disconnected"),
        };

        // Is there a more efficient way of draining the std binary heap?
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let Some(value) = self.max_heap.pop() else {
                break;
            };
            items.push(value);
        }

        self.drain_source
            .send(items)
            .map_err(|_| anyhow!("Drain channel is disconnected"))
    }
}

#[derive(Debug)]
pub struct Queue<T: Debug + Ord> {
    channels: Channels<T>,
}

const RETRY_DELAY: Duration = Duration::from_micros(200);

impl Mempool for Queue<Transaction> {
    /// Tries to submit `tx` to the underlying priority queue.
    /// On error, the [`Transaction`] is dropped and never sent to the queue.
    /// # Note
    /// Future versions can adjust the trait's signature to return the transaction on error or
    /// work with an internal buffer that takes failed transactions and tries to send them at a
    /// later time.
    fn submit(&self, tx: Transaction) {
        if let Err(e) = self.channels.item_source.try_send(tx) {
            match e {
                crossbeam::channel::TrySendError::Full(tx) => {
                    //TODO: Implement exponential backoff
                    // So long, simply try once more
                    std::thread::sleep(RETRY_DELAY);
                    if self.channels.item_source.try_send(tx).is_err() {
                        eprintln!("Error! Cannot submit to queue!");
                    }
                }
                crossbeam::channel::TrySendError::Disconnected(_) => {
                    eprintln!("Error! Cannot submit transaction to queue - it is not listening.");
                }
            }
        }
    }

    fn drain(&self, n: usize) -> Vec<Transaction> {
        if self.channels.drain_command_source.send(n).is_err() {
            eprintln!("Error: Could not drain from queue, the command channel is closed or full!");
        }
        match self.channels.drain_sink.recv() {
            Ok(v) => v,
            Err(_) => {
                eprintln!(
                    "Error: Could not drain from queue, the drain channel is closed or full!"
                );
                vec![]
            }
        }
    }
}

impl Queue<Transaction> {
    pub fn new(capacity: usize) -> Self {
        let channels = StorageFactory::new_queue(capacity);
        Self { channels }
    }

    pub fn stop(self) {
        self.channels.queue_running.store(false, Ordering::Relaxed);
        // Could wait here until the thread is torn down.
    }
}
