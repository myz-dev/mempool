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
| 15        | 5         | 100        | 1173174 / 1170166               | 581 / 6_451                    | ~106_000                                                                             |
| 20        | 1         | 100        | 5302084 / 4455100               | 35 / 1_,_924                   | >400_000                                                                             |

Run tests with:

```shell
cargo run -r -- async -p 20 -c 1 -t 500000
```

The stress test show that this implementation also heavily benefits from less active draining consumer tasks.
