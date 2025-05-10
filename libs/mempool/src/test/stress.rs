use rand::{Rng, rngs::ThreadRng};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::{Mempool, Transaction};

#[derive(Debug, Clone, Copy)]
pub struct StressTestConfig {
    pub num_producers: usize,
    pub num_transactions: usize,
    pub num_consumers: usize,
    pub payload_size_range: (usize, usize),
    pub drain_interval_ms: u64,
    pub drain_batch_size: usize,
    pub gas_price_range: (u64, u64),
    pub run_duration_seconds: u64,
}

impl StressTestConfig {
    /// Creates a randomized [Transaction] within the pre-configured ranges using the passed randomizer `rng`.
    fn randomized_tx(&self, rng: &mut ThreadRng) -> Transaction {
        let payload_size = rng.random_range(self.payload_size_range.0..self.payload_size_range.1);
        let gas_price = rng.random_range(self.gas_price_range.0..self.gas_price_range.1);

        Transaction {
            id: Uuid::new_v4().to_string(),
            gas_price,
            timestamp: Instant::now().elapsed().as_secs(),
            payload: (0..payload_size).map(|_| rng.random::<u8>()).collect(),
        }
    }
}

pub fn run_stress_test<T: Mempool>(mempool: Arc<T>, config: StressTestConfig) -> TestResults {
    println!(
        "Starting stress test with {} producer threads",
        config.num_producers
    );
    println!(
        "Each producer will submit {} transactions",
        config.num_transactions
    );
    println!(
        "Drain interval: {}ms, batch size: {}",
        config.drain_interval_ms, config.drain_batch_size
    );
    println!("\n{:-<75}\n", "");
    let start_time = Instant::now();
    let test_end_time = start_time + Duration::from_secs(config.run_duration_seconds);

    // -- Metrics
    let submitted_count = Arc::new(AtomicUsize::new(0));
    let drained_count = Arc::new(AtomicUsize::new(0));

    // region:    --- Producer
    let producers_stopped = Arc::new(AtomicUsize::new(0));
    let mut producer_handles = vec![];

    for producer_id in 1..=config.num_producers {
        let cloned_pool = Arc::clone(&mempool);
        let cloned_submitted_count = Arc::clone(&submitted_count);
        let cloned_producers_stopped = Arc::clone(&producers_stopped);

        let handle = thread::spawn(move || {
            let mut rng = rand::rng();
            let mut local_submitted = 0;

            while Instant::now() < test_end_time && local_submitted < config.num_transactions {
                let tx = config.randomized_tx(&mut rng);

                // --> Submit
                cloned_pool.submit(tx);
                local_submitted += 1;
                cloned_submitted_count.fetch_add(1, Ordering::Relaxed);

                // Small delay
                thread::sleep(Duration::from_micros(rng.random_range(1..100)));
            }

            cloned_producers_stopped.fetch_add(1, Ordering::SeqCst); // Note: Probably `Relaxed` would be OK also. 
            println!(
                "Producer {} completed, submitted {} transactions",
                producer_id, local_submitted
            );
        });

        producer_handles.push(handle);
    }

    // endregion: --- Producer

    // region:    --- Consumer threads
    let consumer_drained_count = Arc::clone(&drained_count);

    let mut consumer_handles = vec![];

    for consumer_id in 1..=config.num_consumers {
        let cloned_pool = Arc::clone(&mempool);
        let cloned_drained_count = Arc::clone(&consumer_drained_count);
        let cloned_producers_stopped = Arc::clone(&producers_stopped);

        let consumer_handle = thread::spawn(move || {
            let mut total_drained = 0;
            let mut batch_stats = vec![];

            while Instant::now() < test_end_time
                && cloned_producers_stopped.load(Ordering::Relaxed) < config.num_producers
            {
                let drain_start = Instant::now();
                let drained = cloned_pool.drain(config.drain_batch_size);
                let drain_duration = drain_start.elapsed();

                let batch_size = drained.len();
                total_drained += batch_size;
                cloned_drained_count.fetch_add(batch_size, Ordering::Relaxed);

                if batch_size > 0 {
                    // Track batch statistics
                    batch_stats.push(BatchStat {
                        size: batch_size,
                        duration_micros: drain_duration.as_micros() as u64,
                    });
                }

                thread::sleep(Duration::from_millis(config.drain_interval_ms));
            }
            println!(
                "Consumer {:02} completed, drained {} transactions in total",
                consumer_id, total_drained
            );
            batch_stats
        });
        consumer_handles.push(consumer_handle);
    }

    // endregion: --- Consumer threads

    // Wait for producers and consumers
    for handle in producer_handles {
        handle.join().expect("Producer thread panicked");
    }
    println!("Waiting for consumers!");
    let mut batch_stats = vec![];
    for handle in consumer_handles {
        let mut stats = handle.join().expect("Consumer thread panicked");
        batch_stats.append(&mut stats);
    }

    let test_duration = start_time.elapsed();
    let test_duration_ms = test_duration.as_millis();
    assert!(test_duration_ms > 0, "Test should take at least 1ms...");

    // -- Gather metrics
    let total_submitted = submitted_count.load(Ordering::Relaxed);
    let total_drained = drained_count.load(Ordering::Relaxed);

    let transactions_per_second = total_submitted as f64 / (test_duration_ms as f64 / 1000.0);

    let avg_batch_duration_micros = if !batch_stats.is_empty() {
        batch_stats
            .iter()
            .map(|stat| stat.duration_micros)
            .sum::<u64>() as f64
            / batch_stats.len() as f64
    } else {
        0.0
    };

    let avg_batch_size = if !batch_stats.is_empty() {
        (batch_stats.iter().map(|stat| stat.size).sum::<usize>() as f64)
            / (batch_stats.len() as f64)
    } else {
        0.0
    };

    TestResults {
        test_duration,
        total_submitted,
        total_drained,
        transactions_per_second,
        avg_batch_size,
        avg_batch_duration_micros,
        batch_stats,
    }
}

// Structs for storing test results
#[derive(Debug, Clone)]
pub struct BatchStat {
    size: usize,
    duration_micros: u64,
}

#[derive(Debug)]
pub struct TestResults {
    test_duration: Duration,
    total_submitted: usize,
    total_drained: usize,
    transactions_per_second: f64,
    avg_batch_size: f64,
    avg_batch_duration_micros: f64,
    batch_stats: Vec<BatchStat>,
}

impl TestResults {
    pub fn print_summary(&self) {
        println!("\n{:=^75}", " Stress Test Results ");
        println!("Test duration: {:?}", self.test_duration);
        println!("Total transactions submitted: {}", self.total_submitted);
        println!("Total transactions drained: {}", self.total_drained);
        println!(
            "Transactions per second: {:.2}",
            self.transactions_per_second
        );
        println!("Average batch size: {:.2}", self.avg_batch_size);
        println!(
            "Average batch drain duration: {:.2} µs",
            self.avg_batch_duration_micros
        );

        if !self.batch_stats.is_empty() {
            let max_batch_size = self
                .batch_stats
                .iter()
                .map(|stat| stat.size)
                .max()
                .unwrap_or(0);
            let min_batch_size = self
                .batch_stats
                .iter()
                .map(|stat| stat.size)
                .min()
                .unwrap_or(0);
            let max_drain_duration = self
                .batch_stats
                .iter()
                .map(|stat| stat.duration_micros)
                .max()
                .unwrap_or(0);

            println!("\nBatch Statistics:");
            println!(
                "  - Batch size range: {} to {}",
                min_batch_size, max_batch_size
            );
            println!("  - Max drain duration: {} µs", max_drain_duration);
        }
    }
}
