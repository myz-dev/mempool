# Channel based async approach

This repository contains priority pool implementation that utilizes the tokio runtime and tokio message passing channels.

Akin to the synchronous implementation the `async` one spawns a manager task that receives drainage and submittance requests.
The `tokio::select!` based main loop makes for a very efficient hot loop without the need for a `sleep` operation to yield CPU resources when no work is to be done.  

## Design goals

The queue should utilize message passing primitives to enable producer tasks to add to the queue without blocking the execution of the program at all.

The queue's submittance mechanism is based on `tokio`'s `mpsc` channel, that sends its payload without the need to clone the data to the single receiver endpoint. A call to its
`.send()` resolves immediately if the channel has still capacity for new messages. Should the capacity be reached, senders yield back to the `tokio` runtime at this point which 
allows the program to drive other tasks forward while the queue can work on clearing its buffered submissions.

This fact has a nice side-effect: We get a "back-pressure" mechanism for free by using the tokio `mpsc` bounded channel. The channel's ergonomic API can be used to expose different 
push-strategies like waiting for a new slot in the channel's buffer or giving up after a given timeout.

## Stress test results

The async stress test is a little bit more refined than its sync counterpart at the moment.
Generally the `async` versions easily outperforms all other implementations in this repository. Throughput rates of more than 100k transactions per second are easily achieved.

| Producers | Consumers | DrainBatch | Submitted / Drained             | Latency P50.0 / P99.0 [μs]     | Throughput [Transaction per second]                                                  |
| --------  | --------  | --------   | --------                        | --------                       | --------                                                                             |
| 15        | 5         | 100        | 1173174 / 1170166               | 581 / 6_451                    | ~106k                                                                                |
| 20        | 1         | 100        | 5302084 / 4455100               | 35 / 1_,_924                   | >400k                                                                                |

Run tests template:

```shell
cargo run -r -- async -p 20 -c 1 -t 500000
```

The stress test show that this implementation also heavily benefits from less active draining consumer tasks.

## HTTP test results

First tests using the new HTTP test interface show how the queue might perform in a real world usage example.
Here the throughput is dramatically less, which should be caused by the introduced network lag.

| Producers | Consumers | DrainBatch | Submitted / Drained             | Latency P50.0 / P99.0 [μs]     | Throughput [Transaction per second]                                                  |
| --------  | --------  | --------   | --------                        | --------                       | --------                                                                             |
| 10        | 1         | 100        | 128952 / 127776                 | 3,473 / 53_375                 | ~11k                                                                                 |
| 20        | 1         | 100        | 183394 / 182569                 | 3_779 / 39_327                 | ~19k                                                                                 |

Run tests template:

```shell
cargo run -r -- async -p 20 -c 1 -t 500000 --http-port 8080 
```
