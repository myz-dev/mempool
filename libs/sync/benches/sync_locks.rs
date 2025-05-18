use std::hint::black_box;
use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};
use mempool::{Mempool, Transaction};
use sync::LockedQueue;

fn create_tx(gas_price: u64) -> Transaction {
    Transaction {
        id: String::new(),
        gas_price,
        timestamp: Instant::now().elapsed().as_millis() as u64,
        payload: vec![],
    }
}

fn submit_drain(c: &mut Criterion) {
    let pool = LockedQueue::new(50_000);

    c.bench_function("sync_locks submit_drain", |b| {
        b.iter(|| {
            pool.submit(create_tx(black_box(100)));
            let drained = pool.drain(5);
            assert_eq!(drained.len(), 1);
            assert_eq!(drained[0].gas_price, 100);
        })
    });
}

fn submit_high_priority_on_large_queue(c: &mut Criterion) {
    let pool = LockedQueue::new(500_000);
    // -- Prepare large pool
    let mut gas_price = 0;
    for _ in 0..50_000 {
        let tx = create_tx(gas_price);
        pool.submit(black_box(tx));

        gas_price += 1;
    }
    std::thread::sleep(std::time::Duration::from_millis(8_000));
    c.bench_function("sync_locks submit_high_priority_on_large_queue", |b| {
        b.iter(|| {
            let tx = create_tx(black_box(gas_price));
            pool.submit(tx);

            let drained = pool.drain(1);
            assert_eq!(drained[0].gas_price, gas_price); //<-- should equal the last one added (highest gas price)
        });
    });
}

criterion_group!(benches, submit_drain, submit_high_priority_on_large_queue);
criterion_main!(benches);
