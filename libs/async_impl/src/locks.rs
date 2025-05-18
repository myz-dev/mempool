use std::{collections::BinaryHeap, sync::Arc, time::Duration};

use mempool::Transaction;
use tokio::sync::Mutex;

use crate::Mempool;

#[derive(Debug, Clone)]
pub struct LockedQueue {
    pub storage: Arc<Mutex<BinaryHeap<Transaction>>>,
}

impl LockedQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            storage: Arc::new(Mutex::new(BinaryHeap::with_capacity(capacity))),
        }
    }
}

#[async_trait::async_trait]
impl Mempool for LockedQueue {
    async fn submit(&self, tx: Transaction) -> anyhow::Result<()> {
        let mut storage = self.storage.lock().await;
        storage.push(tx);
        Ok(())
    }

    /// Tries to acquire the lock on the internal storage layer and then drains up tp `n` elements from it.
    /// If the lock is not acquired within the `timeout_us` period, an empty vector is returned.
    ///
    /// # Note
    /// The supplied timeout only applies to the time period that is spent waiting for the lock.
    /// It does not account for any additional time that is spent draining the storage layer.
    async fn drain(&self, n: usize, timeout_us: u64) -> anyhow::Result<Vec<Transaction>> {
        let mut interval = tokio::time::interval(Duration::from_micros(timeout_us));
        interval.tick().await; // throw away first immediate tick

        let mut drained_items = Vec::with_capacity(n);
        tokio::select! {
            _ = interval.tick() => {
                // timeout reached
            }
            mut storage = self.storage.lock() => {
                for _ in 0..n {
                    let Some(value) = storage.pop() else {
                        break;
                    };
                    drained_items.push(value);
                }
            }
        }

        Ok(drained_items)
    }
}
