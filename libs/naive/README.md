# Naive vector based queue

The queue is easily characterized by its structure declaration:

```Rust
pub struct NaivePool {
    /// Memory pool that saves the highest priority at the end of the vector, so it can easily be `popped` when drained.
    pool: Mutex<Vec<Transaction>>,
}
```

As the contents of the vector are sorted on submission of new elements, draining `n` elements is as easy as slicing the vector at its tail.
This makes drain operations extremely fast with the trade-off that submitting is relatively slow.

## Performance

The naive implementation performs surprisingly well. Submitting transactions from 10 different threads and draining on two threads leads to a throughput of
~35k on the testing machine.
The highest throughput I could get to was about 43kT/s. This was achieved with the following command: `cargo run -r -- naive -p 20 -c 2 -t 500000`.
