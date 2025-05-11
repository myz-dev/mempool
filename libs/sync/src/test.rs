#[cfg(test)]
mod channel_based_tests {
    use mempool::{Transaction, test::suite};

    use crate::ChanneledQueue;

    struct SyncTester;

    impl suite::Tester<ChanneledQueue<Transaction>> for SyncTester {
        fn create_mempool(&self) -> ChanneledQueue<Transaction> {
            ChanneledQueue::new(500_000)
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

#[cfg(test)]
mod lock_based_tests {
    use mempool::{Transaction, test::suite};

    use crate::LockedQueue;

    struct SyncTester;

    impl suite::Tester<LockedQueue<Transaction>> for SyncTester {
        fn create_mempool(&self) -> LockedQueue<Transaction> {
            LockedQueue::new(500_000)
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
