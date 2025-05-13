use hdrhistogram::Histogram;
use mempool::Transaction;
use rand::Rng;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Barrier, time};

use crate::Mempool;

#[derive(Debug, Clone)]
pub struct StressTestCfg {
    pub num_producers: usize,
    pub num_transactions: usize,
    pub num_consumers: usize,
    pub payload_size_range: (usize, usize),
    pub drain_interval_us: u64,
    pub drain_timeout_us: u64,
    pub drain_batch_size: usize,
    pub gas_price_range: (u64, u64),
    pub run_duration_seconds: u64,
    /// Txs per second, None for max speed
    pub submission_rate: Option<f64>,
    /// Track submission-to-drain latency
    pub latency_tracking: bool,
    /// How often to print stats
    pub print_stats_interval_ms: u64,
    /// Percentiles to track (e.g. [50.0, 90.0, 99.0, 99.9])
    pub latency_percentiles: Vec<f64>,

    pub http_port: Option<u16>,
}

struct TestStats {
    submitted_txs: AtomicU64,
    drained_txs: AtomicU64,
    submit_errors: AtomicU64,
    drain_errors: AtomicU64,
    // Store latencies in a histogram for percentile calculation
    latency_hist: Mutex<Histogram<u64>>,
}

impl TestStats {
    fn new() -> Self {
        Self {
            submitted_txs: AtomicU64::new(0),
            drained_txs: AtomicU64::new(0),
            submit_errors: AtomicU64::new(0),
            drain_errors: AtomicU64::new(0),
            latency_hist: Mutex::new(
                Histogram::new_with_max(60_000_000, 3)
                    .expect("Initializing the histogram should work"),
            ),
        }
    }

    fn record_submission_success(&self) {
        self.submitted_txs.fetch_add(1, Ordering::Relaxed);
    }

    fn record_submission_error(&self) {
        self.submit_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn record_drain_success(&self, count: u64) {
        self.drained_txs.fetch_add(count, Ordering::Relaxed);
    }

    fn record_drain_error(&self) {
        self.drain_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn record_latency(&self, latency_us: u64) {
        // Add to histogram for percentile calculation
        if let Ok(mut hist) = self.latency_hist.lock() {
            let lat = latency_us.min(hist.high());
            hist.record(lat).expect("cannot exceed max");
        }
    }

    // Calculate the specified percentile from the histogram
    fn calculate_percentile(&self, percentile: f64) -> Option<u64> {
        if let Ok(hist) = self.latency_hist.lock() {
            if hist.is_empty() {
                return None;
            }

            return Some(hist.value_at_quantile(percentile / 100.0));
        }
        None
    }

    fn print_stats(&self, elapsed_seconds: f64, percentiles: &[f64]) {
        use num_format::{SystemLocale, ToFormattedString};
        let locale = SystemLocale::default().unwrap();

        let submitted = self.submitted_txs.load(Ordering::Relaxed);
        let drained = self.drained_txs.load(Ordering::Relaxed);
        let sub_errors = self.submit_errors.load(Ordering::Relaxed);
        let drain_errors = self.drain_errors.load(Ordering::Relaxed);

        let submit_rate = submitted as f64 / elapsed_seconds;
        let drain_rate = drained as f64 / elapsed_seconds;

        let avg_latency = { self.latency_hist.lock().map(|h| h.mean()) }.unwrap_or_default();
        let max_latency = { self.latency_hist.lock().map(|h| h.max()) }.unwrap_or_default();

        println!("--- MEMPOOL STATS [{:.2}s] ---", elapsed_seconds);
        println!("Submitted: {} txs ({:.2} txs/sec)", submitted, submit_rate);
        println!("Drained:   {} txs ({:.2} txs/sec)", drained, drain_rate);
        println!("Queue size: ~{} txs", submitted - drained);
        println!("Errors: {} submit, {} drain", sub_errors, drain_errors);

        println!(
            "Latency: avg {} μs, max {} μs.",
            ((avg_latency * 10.0) as u64 / 10).to_formatted_string(&locale),
            max_latency.to_formatted_string(&locale)
        );

        // Print percentiles
        print!("Percentiles: ");
        for &p in percentiles {
            if let Some(latency) = self.calculate_percentile(p) {
                print!("P{:.1}: {} μs, ", p, latency.to_formatted_string(&locale));
            }
        }
        println!();

        println!("---------------------------");
    }
}

async fn run_producer<T: Mempool>(
    queue: T,
    cfg: StressTestCfg,
    stats: Arc<TestStats>,
    start_barrier: Arc<Barrier>,
    stop_signal: Arc<AtomicU64>,
) {
    // Wait for all producers and consumers to be ready
    start_barrier.wait().await;

    let mut tx_counter = 0;

    // Calculate delay between transactions if rate limiting
    let delay = match cfg.submission_rate {
        Some(rate) => {
            // Calculate delay per producer
            let producer_rate = rate / cfg.num_producers as f64;
            Some(Duration::from_secs_f64(1.0 / producer_rate))
        }
        None => None,
    };

    let mut interval = delay.map(time::interval);

    while stop_signal.load(Ordering::Relaxed) == 0 && tx_counter < cfg.num_transactions {
        // If rate limiting is enabled, wait for the next tick
        if let Some(ref mut i) = interval {
            i.tick().await;
        }

        // Generate random transaction
        let tx = {
            let mut rng = rand::rng();
            let gas_price = rng.random_range(cfg.gas_price_range.0..=cfg.gas_price_range.1);
            let payload_size =
                rng.random_range(cfg.payload_size_range.0..=cfg.payload_size_range.1);
            let payload = (0..payload_size).map(|_| rng.random::<u8>()).collect();

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time flowing forwards")
                .as_micros()
                .try_into()
                .expect("conversion okay for the next few years");

            let id = format!("tx-{}", tx_counter);

            Transaction {
                id,
                gas_price,
                timestamp,
                payload,
            }
        };

        match queue.submit(tx).await {
            Ok(_) => {
                stats.record_submission_success();
                tx_counter += 1;
            }
            Err(_) => {
                stats.record_submission_error();
                // Channel is closed, stop producing
                break;
            }
        }
    }
}

async fn run_consumer<T: Mempool>(
    queue: T,
    cfg: StressTestCfg,
    stats: Arc<TestStats>,
    start_barrier: Arc<Barrier>,
    stop_signal: Arc<AtomicU64>,
) {
    // Wait for all producers and consumers to be ready
    start_barrier.wait().await;

    let mut interval = time::interval(Duration::from_micros(cfg.drain_interval_us));

    while stop_signal.load(Ordering::Relaxed) == 0 {
        interval.tick().await;

        let start = Instant::now();
        // Send drain request
        match queue
            .drain(cfg.drain_batch_size, cfg.drain_timeout_us)
            .await
        {
            Ok(txs) => {
                if cfg.latency_tracking && !txs.is_empty() {
                    let delta_us: u64 = start
                        .elapsed()
                        .as_micros()
                        .try_into()
                        .expect("conversion okay for the next few years");

                    stats.record_latency(delta_us);
                }

                stats.record_drain_success(txs.len() as u64);
            }
            Err(_) => {
                stats.record_drain_error();
            }
        }
    }
}

pub async fn run_stress_test<T: Mempool + Clone>(config: StressTestCfg, queue: T) {
    println!("Starting mempool stress test with config: {:?}", config);

    // Create shared stats collector
    let stats = Arc::new(TestStats::new());

    // Start barrier ensures all producers and consumers start simultaneously
    let start_barrier = Arc::new(Barrier::new(
        config.num_producers + config.num_consumers + 1,
    ));

    // Stop signal to coordinate shutdown
    let stop_signal = Arc::new(AtomicU64::new(0));

    // Spawn producers
    let mut producer_handles = Vec::with_capacity(config.num_producers);
    for _ in 0..config.num_producers {
        let producer_queue_handle = queue.clone();
        let producer_stats = Arc::clone(&stats);
        let producer_barrier = Arc::clone(&start_barrier);
        let producer_stop = Arc::clone(&stop_signal);

        let handle = tokio::spawn(run_producer(
            producer_queue_handle,
            config.clone(),
            producer_stats,
            producer_barrier,
            producer_stop,
        ));

        producer_handles.push(handle);
    }

    // Spawn consumers
    let mut consumer_handles = Vec::with_capacity(config.num_consumers);
    for _ in 0..config.num_consumers {
        let consumer_channels = queue.clone();
        let consumer_stats = Arc::clone(&stats);
        let consumer_barrier = Arc::clone(&start_barrier);
        let consumer_stop = Arc::clone(&stop_signal);

        let handle = tokio::spawn(run_consumer(
            consumer_channels,
            config.clone(),
            consumer_stats,
            consumer_barrier,
            consumer_stop,
        ));

        consumer_handles.push(handle);
    }

    // Setup stats printer
    let stats_printer = {
        let stats_clone = Arc::clone(&stats);
        let printer_stop = Arc::clone(&stop_signal);
        let percentiles = config.latency_percentiles.clone();

        tokio::spawn(async move {
            let start_time = Instant::now();
            let mut interval =
                time::interval(Duration::from_millis(config.print_stats_interval_ms));

            while printer_stop.load(Ordering::Relaxed) == 0 {
                interval.tick().await;
                let elapsed = start_time.elapsed().as_secs_f64();
                stats_clone.print_stats(elapsed, &percentiles);
            }

            // Print final stats
            let elapsed = start_time.elapsed().as_secs_f64();
            stats_clone.print_stats(elapsed, &percentiles);
        })
    };

    // Wait for start barrier
    println!("Waiting for all tasks to be ready...");
    start_barrier.wait().await;
    println!("Test started!");

    // Run for specified duration
    time::sleep(Duration::from_secs(config.run_duration_seconds)).await;

    // Signal shutdown
    println!("Test duration completed, shutting down...");
    stop_signal.store(1, Ordering::SeqCst);

    // Wait for all tasks to complete
    for handle in producer_handles {
        let _ = handle.await;
    }

    for handle in consumer_handles {
        let _ = handle.await;
    }

    let _ = stats_printer.await;
}
