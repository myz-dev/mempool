use std::{
    collections::BinaryHeap,
    fmt::Debug,
    sync::{Arc, Mutex},
};

use mempool::{Mempool, Transaction};

#[derive(Debug)]
pub struct LockedQueue<T: Debug + Ord> {
    pub storage: Arc<Mutex<BinaryHeap<T>>>,
}

impl<T: Debug + Ord> LockedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            storage: Arc::new(Mutex::new(BinaryHeap::with_capacity(capacity))),
        }
    }
}

impl Mempool for LockedQueue<Transaction> {
    fn submit(&self, tx: Transaction) {
        let mut storage = self.storage.lock().unwrap();
        storage.push(tx);
    }

    fn drain(&self, n: usize) -> Vec<Transaction> {
        let mut storage = self.storage.lock().unwrap();

        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            let Some(value) = storage.pop() else {
                break;
            };
            items.push(value);
        }

        items
    }
}
