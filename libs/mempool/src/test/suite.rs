use std::{sync::Arc, thread, time::Duration};

use crate::{Mempool, Transaction};

pub trait Tester<T>
where
    T: Mempool,
{
    fn create_mempool(&self) -> T;
}

/// Test basic priority ordering of the [`Mempool`] implementation
pub fn test_ordering_by_gas_price<T: Mempool>(tester: impl Tester<T>) {
    let mempool = tester.create_mempool();

    mempool.submit(Transaction::with_empty_load("tx2", 50, 100));
    mempool.submit(Transaction::with_empty_load("tx5", 20, 200));
    mempool.submit(Transaction::with_empty_load("tx3", 30, 50));
    mempool.submit(Transaction::with_empty_load("tx6", 10, 50));
    mempool.submit(Transaction::with_empty_load("tx4", 20, 50));
    mempool.submit(Transaction::with_empty_load("tx1", 60, 50));

    std::thread::sleep(Duration::from_millis(10)); // wait for all transactions to be harvested by the receiver thread
    let drained = mempool.drain(3);
    assert_eq!(drained.len(), 3);
    assert_eq!(drained[0].id, "tx1");
    assert_eq!(drained[1].id, "tx2");
    assert_eq!(drained[2].id, "tx3");

    let drained = mempool.drain(3);
    assert_eq!(drained.len(), 3);
    assert_eq!(drained[0].id, "tx4");
    assert_eq!(drained[1].id, "tx5");
    assert_eq!(drained[2].id, "tx6");

    let drained = mempool.drain(3);
    assert!(drained.is_empty());
}

pub fn test_concurrent_submit<T: Mempool>(tester: impl Tester<T>) {
    let mempool = Arc::new(tester.create_mempool());

    let mut handles = vec![];

    for i in 0..100 {
        let mempool_clone = mempool.clone();
        let handle = thread::spawn(move || {
            mempool_clone.submit(Transaction::with_empty_load(
                format!("tx{}", i).as_str(),
                i as u64 % 10, // Some variation in gas prices,
                100 + i as u64,
            ));
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let drained = mempool.drain(100);
    assert_eq!(drained.len(), 100);

    // Verify ordering by gas price (descending)
    for window in drained.windows(2) {
        if window[0].gas_price == window[1].gas_price {
            // If gas prices equal, check timestamp ordering (ascending)
            assert!(window[0].timestamp <= window[1].timestamp);
        } else {
            assert!(window[0].gas_price >= window[1].gas_price);
        }
    }
}

pub fn test_concurrent_submit_and_drain<T: Mempool>(tester: impl Tester<T>) {
    let mempool = Arc::new(tester.create_mempool());

    let mut handles = vec![];

    // -- Submit
    for i in 0..50 {
        let mempool_clone = mempool.clone();
        let handle = thread::spawn(move || {
            mempool_clone.submit(Transaction::with_empty_load(
                format!("tx{}", i).as_str(),
                i as u64 % 10,
                100 + i as u64,
            ));
        });
        handles.push(handle);
    }

    // -- Drain
    for _ in 0..5 {
        let mempool_clone = mempool.clone();
        let handle = thread::spawn(move || {
            let drained = mempool_clone.drain(10);
            // Uphold priority ordering
            for window in drained.windows(2) {
                if window[0].gas_price == window[1].gas_price {
                    assert!(window[0].timestamp <= window[1].timestamp);
                } else {
                    assert!(window[0].gas_price >= window[1].gas_price);
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
