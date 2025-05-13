# Lock based and Channel based approach

This repository contains a lock-based and a channel-based implementation of a priority queue.

The lock-based implementation simply wraps the storage layer (a max binary heap) in a `Arc<Mutex<T>>` and hands out the `Arc` to all parties that want to submit to 
or drain from the queue.

The channel-based implementation spawns a manager thread that listens for submit and drain requests and answers them accordingly.
The assumption would be that the lock-based solution might outperform the channel-based implementation in scenarios with low lock contention where the channel management 
overhead does not outweigh any potential waiting for locks to be ready.

## Lock-Based queue

The design goal of the lock based queue is to achieve code simplicity at reasonable performance metrics.

This goal is met with the strikingly simple queue that easily reaches a throughput of ~35k transactions per second.

## Channel-Based queue

The channel based queue aspires to be a more complex implementation that in turn for its complexity outperforms the simple queue in scenarios with high concurrency.
As the underlying memory is never blocked it should constantly serve submittance and drainage requests thereby leading to a better throughput on heavy concurrent loads.

The present implementation does not meet this goal.

## Stress test results

For a test running 10 seconds:

| Producers | Consumers | TxCount  | DrainBatch | Lock Based Produced/Consumed    | Channel Based Produced/Consumed   | Comments                                                                             |
| --------  | --------  | -------- | --------   | --------                        | --------                          | --------                                                                             |
| 15        | 5         | 500_000  | 100        | 325286 / 325286                 | 359400 / 28612                    | Channels could not drain all items in queue                                          |
| 50        | 5         | 500_000  | 100        | 429472 / 429472                 | 429326 / 9640                     | This tests displays that the implementation needs attention at the drain mechanism   |

Run tests template:

```shell
cargo run -r -- sync-channels -p 15 -c 5 -t 500000
```

Or:

```shell
cargo run -r -- sync-locks -p 15 -c 5 -t 500000
```

The data actually shows that the channel based solution is struggling to drain the queue fast enough. As no effort has been spent optimizing this part of the implementation, it remains unclear how much 
potential lies within this approach.
