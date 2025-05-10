#[derive(Debug, Clone, clap::Parser)]
pub struct Cfg {
    /// The memory pool implementation to test.
    pub implementation: Implementation,
    /// Number of Producers that will submit transactions to the memory pool.
    #[arg(short, long)]
    pub producer_num: usize,
    /// Number of transactions each producer will submit to the memory pool during the test.
    #[arg(short, long)]
    pub transaction_num: usize,
    /// Number of Consumers that will drain transactions from the memory pool.
    #[arg(short, long, default_value_t = 1)]
    pub consumer_num: usize,
    /// Delay between the start of each drain interval.
    #[arg(long, default_value_t = 5)]
    pub drain_interval_ms: u64,
    /// Number of transactions that will be drained per batch.
    #[arg(short = 'b', long, default_value_t = 100)]
    pub drain_batch_size: usize,
    // Hard cap on the test's execution time
    #[arg(long, long, default_value_t = 10)]
    pub run_duration_seconds: u64,
}

#[derive(Debug, Clone, strum::EnumString, clap::ValueEnum)]
pub enum Implementation {
    #[strum(ascii_case_insensitive)]
    Naive,
    #[strum(ascii_case_insensitive)]
    Sync,
    #[strum(ascii_case_insensitive)]
    Async,
}
