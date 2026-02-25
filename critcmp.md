# Benchmark Results

Collected on NVMe storage with `cargo bench --bench throughput` and
[critcmp](https://github.com/BurntSushi/critcmp).

```
group                                                  current
-----                                                  -------
batch_iter/1M_records_batch1024/1024                   1.00     85.9±0.23ms   11.1 MElem/sec
batch_iter/1M_records_batch16384/16384                 1.00     85.8±0.07ms   11.1 MElem/sec
batch_iter/1M_records_batch64/64                       1.00     86.2±0.55ms   11.1 MElem/sec
bulk_write/records/1M                                  1.00    571.4±3.44ms   1709.1 KElem/sec
concurrent_random_read/16t_x_100k_gets_in_10M/1600k    1.00     26.6±0.53ms   57.3 MElem/sec
concurrent_random_read/1t_x_100k_gets_in_10M/100k      1.00      3.9±0.01ms   24.6 MElem/sec
concurrent_random_read/2t_x_100k_gets_in_10M/200k      1.00      6.1±0.04ms   31.2 MElem/sec
concurrent_random_read/4t_x_100k_gets_in_10M/400k      1.00      9.2±0.22ms   41.5 MElem/sec
concurrent_random_read/8t_x_100k_gets_in_10M/800k      1.00     14.0±0.10ms   54.7 MElem/sec
concurrent_sequential_read/2t_x_1M_records/2M          1.00    105.9±0.87ms   18.0 MElem/sec
concurrent_sequential_read/4t_x_1M_records/4M          1.00    109.7±3.87ms   34.8 MElem/sec
concurrent_sequential_read/8t_x_1M_records/8M          1.00    118.2±0.85ms   64.6 MElem/sec
concurrent_write/2t_x_1M_records/2M                    1.00    689.9±2.29ms    2.8 MElem/sec
concurrent_write/4t_x_1M_records/4M                    1.00   726.3±94.03ms    5.3 MElem/sec
concurrent_write/8t_x_1M_records/8M                    1.00   1168.2±5.22ms    6.5 MElem/sec
random_read/10k_gets_in/10M                            1.00      2.1±0.02ms    4.6 MElem/sec
random_read/10k_gets_in/1M                             1.00   341.3±15.38µs   27.9 MElem/sec
read_all_to_sink/records/10M                           1.00    505.5±0.63ms   18.9 MElem/sec
read_all_to_sink/records/1M                            1.00     45.6±0.45ms   20.9 MElem/sec
read_to_sink_async/records/10M                         1.00    820.6±6.82ms   11.6 MElem/sec
read_to_sink_async/records/1M                          1.00     73.7±0.52ms   12.9 MElem/sec
sequential_read/records/10M                            1.00  1074.5±22.35ms    8.9 MElem/sec
sequential_read/records/1M                             1.00     94.5±0.71ms   10.1 MElem/sec
write/records/10M                                      1.00       6.9±0.05s 1419.9 KElem/sec
write/records/1M                                       1.00    681.3±7.81ms 1433.5 KElem/sec
write_from_iter_async/records/10M                      1.00       6.9±0.00s 1414.7 KElem/sec
write_from_iter_async/records/1M                       1.00    689.3±1.02ms 1416.7 KElem/sec
```