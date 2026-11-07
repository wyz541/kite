# Kite 0.6.61 performance baseline

- Commit: `ae82ed2f682f861a2f50704bf1f62e91be437274`
- Profile: `standard`
- Build: `release`
- Generated: `2026-11-07T04:05:38.374Z` UTC
- Raw samples: [v0.6.61-macos-arm64.json](results/v0.6.61-macos-arm64.json)
- Reproducibility: [second run and variance report](reproducibility-v0.6.61.md)
- Cross-platform CI baseline: [Windows and macOS](baseline-v0.6.61-ci.md)

## Environment

| Field | Value |
| --- | --- |
| OS | macOS 27.0.1 |
| Architecture | aarch64 |
| CPU | Apple M1 Max |
| Logical cores | 10 |
| Memory | 64.00 GiB |
| Filesystem | apfs |
| Disk kind | SSD |
| Disk free at start | 419.12 GiB |

## Coverage

| Area | Status | Automated scenarios | Gaps |
| --- | --- | --- | --- |
| Startup and launch | not_automated | None | release EXE cold/warm start, CLI/default-handler launch, and first paint<br>release app cold/warm start, Finder/default-handler launch, and first paint |
| Column navigation | partial | load_directory_1000_name_sort<br>load_directory_10000_name_sort<br>load_directory_50000_name_sort<br>sort_entries_50000_name<br>sort_entries_50000_modified<br>sort_entries_50000_kind<br>sort_entries_50000_size<br>sort_entries_50000_vcs | native GPUI next-column, filter, group, and frame-stall measurement<br>native SwiftUI column rendering and frame-stall measurement |
| Tabs and session restore | partial | load_session_20_tabs<br>save_session_20_tabs | native GPUI tab open, switch, close, and restore-render measurement<br>native SwiftUI tab open, switch, close, and restore-render measurement |
| Directory watch and refresh | not_automated | None | Swift DispatchSource event-to-refresh and frame-stall measurement |
| Column search | partial | search_directory_10000<br>search_directory_100000 | GPUI search-field feedback and frame-stall measurement<br>SwiftUI search-field feedback and frame-stall measurement |
| Global name search | measured | global_name_search_10000<br>global_name_search_100000<br>global_name_search_multi_root | None |
| Global content search | partial | cold_content_index_build<br>warm_content_query<br>incremental_update_to_query | GPUI result-card rendering and frame-stall measurement<br>SwiftUI result-card rendering and frame-stall measurement |
| Preview and editor | partial | text_open_1mib<br>text_open_8mib<br>text_open_100mib | GPUI image, Markdown, code, and log rendering plus scroll-stall measurement<br>Quick Look, SwiftUI preview, NSTextView, and scroll-stall measurement |
| File operations | partial | create_100_files<br>rename_file<br>copy_small_file_tree<br>move_small_file_tree<br>copy_large_file | GPUI progress feedback, recycle-bin action, and Explorer clipboard integration<br>SwiftUI progress feedback, Finder clipboard integration, and trash action |
| Archive operations | partial | compress_zip<br>compress_tar_gz<br>extract_zip<br>extract_tar_gz<br>compress_large_file_zip | GPUI archive progress and frame-stall measurement<br>SwiftUI archive progress and frame-stall measurement |
| VCS and context menus | partial | git_clean_status<br>git_dirty_status_100_files<br>apply_vcs_statuses | Windows native Shell context-menu and third-party extension latency<br>AppKit menu latency and SVN fixture when svn is unavailable<br>SVN was not available in the benchmark environment |
| Sidebar and devices | partial | volume_enumeration | WPD/MTP first listing, navigation, and copy throughput with a controlled device<br>USB volume first listing and copy throughput with a controlled device |
| Update path | partial | sha256_verify_large_file | GitHub Release API, download throughput, installer, and restart with --include-network |

## Results

| Area | Scenario | Boundary | n | p50 ms | p95 ms | max ms | Throughput p50 | Peak RSS delta |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Column navigation | load_directory_1000_name_sort | core | 30 | 3.349 | 3.864 | 4.023 | 298643.10 items/s | 0.05 MiB |
| Column navigation | load_directory_10000_name_sort | core | 30 | 39.933 | 41.894 | 43.144 | 250419.57 items/s | 0.09 MiB |
| Column navigation | load_directory_50000_name_sort | core | 30 | 216.001 | 224.059 | 230.495 | 231479.93 items/s | 5.02 MiB |
| Column navigation | sort_entries_50000_name | core | 30 | 6.125 | 6.194 | 6.228 | 8163099.73 items/s | 0.02 MiB |
| Column navigation | sort_entries_50000_modified | core | 30 | 3.733 | 3.783 | 3.865 | 13392575.82 items/s | 0.02 MiB |
| Column navigation | sort_entries_50000_kind | core | 30 | 6.349 | 6.402 | 6.421 | 7875514.65 items/s | 0.02 MiB |
| Column navigation | sort_entries_50000_size | core | 30 | 3.411 | 3.431 | 3.455 | 14656578.18 items/s | 0.02 MiB |
| Column navigation | sort_entries_50000_vcs | core | 30 | 3.434 | 3.489 | 3.503 | 14560105.72 items/s | 0.02 MiB |
| Column navigation | native GPUI next-column, filter, group, and frame-stall measurement | windows_runtime | 0 | - | - | - | - | not_automated |
| Column navigation | native SwiftUI column rendering and frame-stall measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| Tabs and session restore | load_session_20_tabs | core | 30 | 0.032 | 0.047 | 0.051 | 624596.70 items/s | 0.02 MiB |
| Tabs and session restore | save_session_20_tabs | core | 30 | 5.372 | 5.907 | 5.930 | 3723.00 items/s | 0.03 MiB |
| Tabs and session restore | native GPUI tab open, switch, close, and restore-render measurement | windows_runtime | 0 | - | - | - | - | not_automated |
| Tabs and session restore | native SwiftUI tab open, switch, close, and restore-render measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| Directory watch and refresh | Swift DispatchSource event-to-refresh and frame-stall measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| Column search | search_directory_10000 | core | 30 | 7.166 | 8.856 | 9.832 | 1395461.04 items/s | 0.06 MiB |
| Column search | search_directory_100000 | core | 30 | 57.952 | 63.782 | 65.437 | 1725567.44 items/s | 0.14 MiB |
| Global name search | global_name_search_10000 | core | 30 | 9.794 | 10.124 | 10.163 | 1021076.00 items/s | 0.03 MiB |
| Global name search | global_name_search_100000 | core | 30 | 96.899 | 100.043 | 100.381 | 1032005.06 items/s | 0.05 MiB |
| Global name search | global_name_search_multi_root | core | 30 | 47.709 | 48.912 | 49.005 | 1257629.89 items/s | 0.06 MiB |
| Column search | GPUI search-field feedback and frame-stall measurement | windows_runtime | 0 | - | - | - | - | not_automated |
| Column search | SwiftUI search-field feedback and frame-stall measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| Global content search | cold_content_index_build | core | 10 | 3501.483 | 3554.824 | 3563.160 | 571.19 items/s | 14.38 MiB |
| Global content search | warm_content_query | core | 30 | 11.752 | 12.153 | 12.408 | 4254.43 items/s | 0.03 MiB |
| Global content search | incremental_update_to_query | core | 10 | 196.807 | 202.207 | 204.928 | 5.08 items/s | 0.34 MiB |
| Global content search | GPUI result-card rendering and frame-stall measurement | windows_runtime | 0 | - | - | - | - | not_automated |
| Global content search | SwiftUI result-card rendering and frame-stall measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| Preview and editor | text_open_1mib | core | 10 | 3.976 | 4.029 | 4.043 | 251.49 MiB/s | 0.02 MiB |
| Preview and editor | text_open_8mib | core | 10 | 32.039 | 32.493 | 32.552 | 249.70 MiB/s | 0.11 MiB |
| Preview and editor | text_open_100mib | core | 10 | 0.005 | 0.008 | 0.008 | 218487.80 items/s | 0.02 MiB |
| Preview and editor | GPUI image, Markdown, code, and log rendering plus scroll-stall measurement | windows_runtime | 0 | - | - | - | - | not_automated |
| Preview and editor | Quick Look, SwiftUI preview, NSTextView, and scroll-stall measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| File operations | create_100_files | core | 30 | 5.557 | 23.175 | 34.560 | 17996.60 items/s | 0.11 MiB |
| File operations | rename_file | core | 30 | 0.136 | 0.163 | 0.173 | 7338.33 items/s | 0.02 MiB |
| File operations | copy_small_file_tree | core | 30 | 131.438 | 143.121 | 144.247 | 7608.17 items/s | 0.02 MiB |
| File operations | move_small_file_tree | core | 10 | 0.358 | 0.446 | 0.466 | 279187.14 items/s | 0.02 MiB |
| File operations | copy_large_file | core | 10 | 0.321 | 0.385 | 0.392 | 799006.44 MiB/s | 0.02 MiB |
| File operations | GPUI progress feedback, recycle-bin action, and Explorer clipboard integration | windows_runtime | 0 | - | - | - | - | not_automated |
| File operations | SwiftUI progress feedback, Finder clipboard integration, and trash action | macos_runtime | 0 | - | - | - | - | not_automated |
| Archive operations | compress_zip | core | 10 | 107.493 | 111.221 | 111.627 | 18606.45 items/s | 0.02 MiB |
| Archive operations | compress_tar_gz | core | 10 | 54.296 | 55.604 | 56.448 | 36835.37 items/s | 0.02 MiB |
| Archive operations | extract_zip | core | 10 | 191.621 | 202.628 | 207.891 | 10437.86 items/s | 0.23 MiB |
| Archive operations | extract_tar_gz | core | 10 | 286.540 | 324.586 | 327.403 | 6979.87 items/s | 0.03 MiB |
| Archive operations | compress_large_file_zip | core | 5 | 5313.858 | 5509.136 | 5555.974 | 48.18 MiB/s | 0.02 MiB |
| Archive operations | GPUI archive progress and frame-stall measurement | windows_runtime | 0 | - | - | - | - | not_automated |
| Archive operations | SwiftUI archive progress and frame-stall measurement | macos_runtime | 0 | - | - | - | - | not_automated |
| VCS and context menus | git_clean_status | core | 10 | 66.090 | 71.081 | 72.344 | 151311.44 items/s | 0.05 MiB |
| VCS and context menus | git_dirty_status_100_files | core | 10 | 66.490 | 67.973 | 68.204 | 150408.79 items/s | 0.02 MiB |
| VCS and context menus | apply_vcs_statuses | core | 30 | 1.700 | 1.765 | 1.774 | 5883735.24 items/s | 0.05 MiB |
| VCS and context menus | Windows native Shell context-menu and third-party extension latency | windows_runtime | 0 | - | - | - | - | not_automated |
| VCS and context menus | AppKit menu latency and SVN fixture when svn is unavailable | macos_runtime | 0 | - | - | - | - | not_automated |
| Sidebar and devices | volume_enumeration | core | 30 | 55.992 | 84.666 | 129.564 | 35.72 items/s | 0.08 MiB |
| Sidebar and devices | WPD/MTP first listing, navigation, and copy throughput with a controlled device | windows_runtime | 0 | - | - | - | - | not_automated |
| Sidebar and devices | USB volume first listing and copy throughput with a controlled device | macos_runtime | 0 | - | - | - | - | not_automated |
| Update path | sha256_verify_large_file | core | 5 | 816.452 | 817.326 | 817.390 | 313.55 MiB/s | 0.02 MiB |
| Update path | GitHub Release API, download throughput, installer, and restart with --include-network | external | 0 | - | - | - | - | not_automated |
| Startup and launch | release EXE cold/warm start, CLI/default-handler launch, and first paint | windows_runtime | 0 | - | - | - | - | not_automated |
| Startup and launch | release app cold/warm start, Finder/default-handler launch, and first paint | macos_runtime | 0 | - | - | - | - | not_automated |
| VCS and context menus | SVN was not available in the benchmark environment | core | 0 | - | - | - | - | not_automated |

## Slowest scenarios

| Rank | Area | Scenario | p95 ms | n |
| ---: | --- | --- | ---: | ---: |
| 1 | Archive operations | compress_large_file_zip | 5509.136 | 5 |
| 2 | Global content search | cold_content_index_build | 3554.824 | 10 |
| 3 | Update path | sha256_verify_large_file | 817.326 | 5 |
| 4 | Archive operations | extract_tar_gz | 324.586 | 10 |
| 5 | Column navigation | load_directory_50000_name_sort | 224.059 | 30 |
| 6 | Archive operations | extract_zip | 202.628 | 10 |
| 7 | Global content search | incremental_update_to_query | 202.207 | 10 |
| 8 | File operations | copy_small_file_tree | 143.121 | 30 |
| 9 | Archive operations | compress_zip | 111.221 | 10 |
| 10 | Global name search | global_name_search_100000 | 100.043 | 30 |
| 11 | Sidebar and devices | volume_enumeration | 84.666 | 30 |
| 12 | VCS and context menus | git_clean_status | 71.081 | 10 |

## Findings

- Largest measured p95: `Archive operations / compress_large_file_zip` at 5509.136 ms.
- Highest timing variance: `File operations / create_100_files` with p95/p50 ratio 4.17.
- Largest measured RSS delta: `Global content search / cold_content_index_build` at 14.38 MiB.
- Native or environment coverage remains incomplete for: Startup and launch, Column navigation, Tabs and session restore, Directory watch and refresh, Column search, Global content search, Preview and editor, File operations, Archive operations, VCS and context menus, Sidebar and devices, Update path.

## Highest variance

| Rank | Area | Scenario | p95/p50 | n |
| ---: | --- | --- | ---: | ---: |
| 1 | File operations | create_100_files | 4.17 | 30 |
| 2 | Preview and editor | text_open_100mib | 1.70 | 10 |
| 3 | Sidebar and devices | volume_enumeration | 1.51 | 30 |
| 4 | Tabs and session restore | load_session_20_tabs | 1.47 | 30 |
| 5 | File operations | move_small_file_tree | 1.25 | 10 |
| 6 | Column search | search_directory_10000 | 1.24 | 30 |
| 7 | File operations | copy_large_file | 1.20 | 10 |
| 8 | File operations | rename_file | 1.20 | 30 |

## Highest RSS delta

| Rank | Area | Scenario | Peak RSS delta | n |
| ---: | --- | --- | ---: | ---: |
| 1 | Global content search | cold_content_index_build | 14.38 MiB | 10 |
| 2 | Column navigation | load_directory_50000_name_sort | 5.02 MiB | 30 |
| 3 | Global content search | incremental_update_to_query | 0.34 MiB | 10 |
| 4 | Archive operations | extract_zip | 0.23 MiB | 10 |
| 5 | Column search | search_directory_100000 | 0.14 MiB | 30 |
| 6 | Preview and editor | text_open_8mib | 0.11 MiB | 10 |
| 7 | File operations | create_100_files | 0.11 MiB | 30 |
| 8 | Column navigation | load_directory_10000_name_sort | 0.09 MiB | 30 |

## Method

- Timer: std::time::Instant (monotonic)
- Percentile: linear interpolation over sorted samples
- RSS: sysinfo resident memory sampled every 10 ms; peak is max of sampler and post-sample RSS
- Cache control: fixtures are generated locally; the harness does not purge OS caches
- Sample counts: 30 interactive, 10 cold/build
