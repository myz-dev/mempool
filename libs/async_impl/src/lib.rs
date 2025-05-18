use mempool::Transaction;

mod channels;
mod locks;

pub use channels::drain_strategy;
pub use channels::stress::{HttpFacade, StressTestCfg, run_stress_test};
pub use channels::worker;
pub use locks::LockedQueue;

#[async_trait::async_trait]
pub trait Mempool: Send + Sync + 'static {
    async fn submit(&self, tx: Transaction) -> anyhow::Result<()>;
    async fn drain(&self, n: usize, timeout_us: u64) -> anyhow::Result<Vec<Transaction>>;
}
