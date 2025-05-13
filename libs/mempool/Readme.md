# Mempool

This core package contains the `Mempool` trait that serves as the basis of the synchronous queue implementations.

It also contains a test suite and the stress test that are designed against the interface of `Mempool` in order to share one test suite across 
different implementations.

## Test suite

The test suite is used for different implementations and tests correct prioritization of incoming and outgoing transactions.

## Stress test

The stress test is designed to test synchronous implementations under heavy load and collect data that can be used to judge the quality of each implementation.

The stress test should be improved to collect more metrics (like latency percentiles, number of drainage operations etc.).
