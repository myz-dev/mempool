#[cfg(test)]
mod test_suite {
    use mempool::{Transaction, test::suite};

    use crate::Queue;

    struct SyncTester;

    impl suite::Tester<Queue<Transaction>> for SyncTester {
        fn create_mempool(&self) -> Queue<Transaction> {
            Queue::new(500_000)
        }
    }

    #[test]
    fn ordering_by_gas_price() {
        suite::test_ordering_by_gas_price(SyncTester)
    }

    #[test]
    fn concurrent_submit() {
        suite::test_concurrent_submit(SyncTester);
    }

    #[test]
    fn concurrent_submit_and_drain() {
        suite::test_concurrent_submit_and_drain(SyncTester);
    }
}
