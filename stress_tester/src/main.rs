use cfg::Cfg;
use clap::Parser;
use mempool::test::stress::{StressTestConfig, run_stress_test};
use naive::NaivePool;
use sync::Queue;

pub mod cfg;

fn main() {
    let cfg = cfg::Cfg::parse();
    println!("Running configuration:\n{cfg:#?}");

    let res = match cfg.implementation {
        cfg::Implementation::Naive => run_naive(cfg),
        cfg::Implementation::Sync => run_sync_channels(cfg),
        cfg::Implementation::Async => run_async(cfg),
    };
    if let Err(e) = res {
        eprintln!("Error: {e:?}");
    }
}

fn run_naive(cfg: Cfg) -> anyhow::Result<()> {
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
    use std::sync::Arc;

    let capacity = cfg
        .transaction_num
        .checked_mul(cfg.producer_num)
        .ok_or_else(|| anyhow::anyhow!("Overflow while calculating mempool capacity"))?;

    let mempool = Arc::new(Queue::new(capacity));
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
    todo!()
}
