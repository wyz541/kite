# Kite startup performance

- Platform: `darwin`
- App: `Kite.app`
- Versions: `0.6.61`
- Fixture entries: `10000`
- Target event: `macos.first_content`
- OS: `27.0.1` (`arm64`)
- CPU: `Apple M1 Max`
- Memory: `68719476736` bytes

| Metric | p50 ms | p95 ms | max ms | n |
| --- | ---: | ---: | ---: | ---: |
| Launch to first content | 809.000 | 817.650 | 819.000 | 10 |
| Process entry to first content | 729.500 | 735.550 | 736.000 | 10 |

Peak RSS: `166445056` bytes.

### By phase

| Phase | n | Launch p50 ms | Launch p95 ms | Entry p50 ms | Entry p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| first_process | 1 | 795.000 | 795.000 | 720.000 | 720.000 |
| warm_restart | 9 | 810.000 | 817.800 | 730.000 | 735.600 |

## Runs

| Run | Phase | Launch to content ms | Entry to content ms | Peak RSS bytes |
| ---: | --- | ---: | ---: | ---: |
| 0 | first_process | 795.000 | 720.000 | 164446208 |
| 1 | warm_restart | 808.000 | 730.000 | 159432704 |
| 2 | warm_restart | 801.000 | 720.000 | 163332096 |
| 3 | warm_restart | 819.000 | 728.000 | 160497664 |
| 4 | warm_restart | 807.000 | 733.000 | 161120256 |
| 5 | warm_restart | 816.000 | 736.000 | 166445056 |
| 6 | warm_restart | 810.000 | 731.000 | 161480704 |
| 7 | warm_restart | 806.000 | 726.000 | 162562048 |
| 8 | warm_restart | 815.000 | 735.000 | 165412864 |
| 9 | warm_restart | 813.000 | 729.000 | 164675584 |

## Boundary

- first SwiftUI KiteColumnBrowser onAppear after the initial directory is loaded.
- The result is a first-content proxy, not a compositor-confirmed first frame.
- OS caches are not purged. Run 0 is the first process only; later runs use warm caches.
- Native interaction frame stalls remain outside this runner.
