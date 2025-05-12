//! A priority queue implementation that relies on passing memory through synchronization channels
//! instead of lock-coordinated direct memory access.

pub mod drain_strategy;
pub mod stress;
pub mod worker;
