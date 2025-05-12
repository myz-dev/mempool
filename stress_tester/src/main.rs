use cfg::Cfg;
use clap::Parser;
use naive::NaivePool;
use sync::{ChanneledQueue, LockedQueue};

pub mod cfg;

fn main() {
    let cfg = cfg::Cfg::parse();
    println!("Running configuration:\n{cfg:#?}");

    let res = match cfg.implementation {
        cfg::Implementation::Naive => run_naive(cfg),
        cfg::Implementation::SyncChannels => run_sync_channels(cfg),
        cfg::Implementation::SyncLocks => run_sync_lock_based(cfg),
        cfg::Implementation::Async => run_async(cfg),
    };
    if let Err(e) = res {
        eprintln!("Error: {e:?}");
    }
}

fn run_naive(cfg: Cfg) -> anyhow::Result<()> {
    use mempool::test::stress::{StressTestConfig, run_stress_test};
    use std::sync::Arc;

    let capacity = cfg
        .transaction_num
        .checked_mul(cfg.producer_num)
        .ok_or_else(|| anyhow::anyhow!("Overflow while calculating mempool capacity"))?;

    let mempool = Arc::new(NaivePool::new(capacity));
    let config = StressTestConfig {
        num_producers: cfg.producer_num,
        num_transactions: cfg.transaction_num,
        num_consumers: 1,
        payload_size_range: (256, 1_024),
        drain_interval_ms: cfg.drain_interval_ms,
        drain_batch_size: cfg.drain_batch_size,
        gas_price_range: (142, 654),
        run_duration_seconds: cfg.run_duration_seconds,
    };
    let results = run_stress_test(mempool, config);
    results.print_summary();

    Ok(())
}

fn run_sync_channels(cfg: Cfg) -> anyhow::Result<()> {
    use mempool::test::stress::{StressTestConfig, run_stress_test};
    use std::sync::Arc;

    let capacity = cfg
        .transaction_num
        .checked_mul(cfg.producer_num)
        .ok_or_else(|| anyhow::anyhow!("Overflow while calculating mempool capacity"))?;

    let mempool = Arc::new(ChanneledQueue::new(capacity));
    let config = StressTestConfig {
        num_producers: cfg.producer_num,
        num_transactions: cfg.transaction_num,
        num_consumers: cfg.consumer_num,
        payload_size_range: (256, 1_024),
        drain_interval_ms: cfg.drain_interval_ms,
        drain_batch_size: cfg.drain_batch_size,
        gas_price_range: (142, 654),
        run_duration_seconds: cfg.run_duration_seconds,
    };
    let results = run_stress_test(mempool, config);
    results.print_summary();
    Ok(())
}

fn run_sync_lock_based(cfg: Cfg) -> anyhow::Result<()> {
    use mempool::test::stress::{StressTestConfig, run_stress_test};
    use std::sync::Arc;

    let capacity = cfg
        .transaction_num
        .checked_mul(cfg.producer_num)
        .ok_or_else(|| anyhow::anyhow!("Overflow while calculating mempool capacity"))?;

    let mempool = Arc::new(LockedQueue::new(capacity));
    let config = StressTestConfig {
        num_producers: cfg.producer_num,
        num_transactions: cfg.transaction_num,
        num_consumers: cfg.consumer_num,
        payload_size_range: (256, 1_024),
        drain_interval_ms: cfg.drain_interval_ms,
        drain_batch_size: cfg.drain_batch_size,
        gas_price_range: (142, 654),
        run_duration_seconds: cfg.run_duration_seconds,
    };
    let results = run_stress_test(mempool, config);
    results.print_summary();
    Ok(())
}

fn run_async(_cfg: Cfg) -> anyhow::Result<()> {
    use async_impl::{StressTestCfg, run_stress_test};

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let cfg = StressTestCfg {
            num_producers: 4,
            num_transactions: 1_000_000,
            num_consumers: 2,
            payload_size_range: (100, 1000),
            drain_interval_us: 100,
            drain_batch_size: 500,
            drain_timeout_us: 3_000,
            gas_price_range: (1, 1000),
            run_duration_seconds: 30,
            submission_rate: None, // Max speed
            latency_tracking: true,
            print_stats_interval_ms: 1000,
            latency_percentiles: vec![50.0, 90.0, 99.0, 99.9],
        };
        let queue_cfg = async_impl::worker::Cfg {
            capacity: cfg.num_producers * cfg.num_transactions,
            submittance_back_pressure: 3_000,
        };
        let queue = async_impl::worker::Queue::start(queue_cfg);
        run_stress_test(cfg, queue.clone()).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        queue.stop()
    });
    Ok(())
}
