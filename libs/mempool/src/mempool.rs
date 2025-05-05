use std::cmp::Ordering;

pub trait Mempool: Send + Sync + 'static {
    fn submit(&self, tx: Transaction);
    fn drain(&self, n: usize) -> Vec<Transaction>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: String,
    pub gas_price: u64,
    pub timestamp: u64,
    pub payload: Vec<u8>,
}

impl Transaction {
    /// As defined in the assignment, priority is determined using the following criteria:
    /// - Higher gas prices lead to a higher priority.
    /// - On equal gas price, an earlier timestamp leads to a higher priority.
    fn priority(&self, other: &Self) -> Ordering {
        if self.gas_price != other.gas_price {
            return self.gas_price.cmp(&other.gas_price);
        }
        other.timestamp.cmp(&self.timestamp)
    }

    pub fn new(id: &str, gas_price: u64, timestamp: u64, payload: Vec<u8>) -> Self {
        Self {
            id: id.to_string(),
            gas_price,
            timestamp,
            payload,
        }
    }

    pub fn without_load(id: &str, gas_price: u64, timestamp: u64) -> Self {
        Self {
            id: id.to_string(),
            gas_price,
            timestamp,
            payload: vec![],
        }
    }
}

// region:    --- Implementation of ordering traits to support sorting by priority

impl PartialOrd for Transaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Transaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority(other)
    }
}

// endregion: --- Section

#[cfg(test)]
mod tests {
    use super::Transaction;
    use std::cmp::Ordering;

    /// Higher gas price -> Higher priority
    #[test]
    fn cmp_diff_gas_price() {
        let low = Transaction::without_load("low", 10, 100);
        let high = Transaction::without_load("high", 20, 50);

        assert_eq!(low.cmp(&high), Ordering::Less);
        assert_eq!(high.cmp(&low), Ordering::Greater);
    }

    /// On same gas price, earlier timestamp has higher priority
    #[test]
    fn cmp_same_gas_diff_timestamp() {
        let early = Transaction::without_load("early", 10, 100);
        let late = Transaction::without_load("late", 10, 200);

        assert_eq!(early.cmp(&late), Ordering::Greater);
        assert_eq!(late.cmp(&early), Ordering::Less);
    }

    /// Equal priority conditions lead to an `Ordering::Equal` evaluation, so in these cases
    /// sorting becomes a no-op.
    #[test]
    fn cmp_ordering_equal_tx() {
        let a = Transaction::without_load("a", 10, 100);
        let b = Transaction::without_load("b", 10, 100);

        assert_eq!(a.cmp(&b), Ordering::Equal);
        assert_eq!(b.partial_cmp(&a), Some(Ordering::Equal));
    }

    #[test]
    fn sort_transactions() {
        let mut txs = vec![
            Transaction::without_load("t1", 5, 100), // -- lowest price, recent addition
            Transaction::without_load("t2", 5, 300), // -- lowest price, late addition
            Transaction::without_load("t3", 20, 50), // -- highest price
            Transaction::without_load("t4", 10, 200), // -- second highest price
        ];
        txs.sort();

        let ids: Vec<&str> = txs.iter().map(|tx| tx.id.as_str()).collect();
        assert_eq!(ids, vec!["t2", "t1", "t4", "t3"]);
    }
}
