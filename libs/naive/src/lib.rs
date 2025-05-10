use std::sync::Mutex;

use mempool::{Mempool, Transaction};

/// Naive implementation of a memory pool that just organizes all elements linearly within a vector.
/// No optimizations are attempted with this implementation.
pub struct NaivePool {
    /// Memory pool that saves the highest priority at the end of the vector, so it can easily be `popped` when drained.
    pool: Mutex<Vec<Transaction>>,
}

impl NaivePool {
    pub fn new(capacity: usize) -> Self {
        Self {
            pool: Mutex::new(Vec::with_capacity(capacity)),
        }
    }
}

impl Mempool for NaivePool {
    /// Very naive and expensive addition to the queue (~O(n) due to call to vector sort on every insert).
    fn submit(&self, tx: Transaction) {
        let mut guard = self.pool.lock().unwrap();
        guard.push(tx);
        guard.sort();
    }

    fn drain(&self, n: usize) -> Vec<Transaction> {
        let mut guard = self.pool.lock().unwrap();

        let drain_start = guard.len().saturating_sub(n);

        let mut drained = guard.split_off(drain_start);
        drained.reverse(); // bring highest priority to the front
        drained
    }
}

#[cfg(test)]
mod test_suite {
    use mempool::test::suite;

    use super::NaivePool;

    struct NaiveTester;

    impl suite::Tester<NaivePool> for NaiveTester {
        fn create_mempool(&self) -> NaivePool {
            NaivePool::new(50000)
        }
    }

    #[test]
    fn ordering_by_gas_price() {
        suite::test_ordering_by_gas_price(NaiveTester);
    }

    #[test]
    fn concurrent_submit() {
        suite::test_concurrent_submit(NaiveTester);
    }

    #[test]
    fn concurrent_submit_and_drain() {
        suite::test_concurrent_submit_and_drain(NaiveTester);
    }
}
