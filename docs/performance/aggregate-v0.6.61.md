# Kite aggregated performance baseline

- Version: `0.6.61`
- Commit: `d2ee7335cc7ecf78a4419a31d6cfe058025a3aea`
- Profile: `standard`
- Build: `release`
- Independent runs: `3`
- Minimum runs: `3`

## Noise summary

| Noise | Scenarios |
| --- | ---: |
| low | 22 |
| medium | 9 |
| high | 5 |
| insufficient_runs | 0 |

Per-run p95 spread is the range divided by the median. The suggested comparison threshold is the larger of 10% and the observed p95 spread.

## Results

| Area | Scenario | p50 median ms | p95 median ms | p95 spread | Noise | Suggested delta threshold | Peak RSS max |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| Archive operations | compress_large_file_zip | 5100.877 | 5123.224 | 0.2% | low | 10.0% | 0.02 MiB |
| Archive operations | compress_tar_gz | 49.719 | 52.858 | 6.2% | low | 10.0% | 0.02 MiB |
| Archive operations | compress_zip | 95.395 | 98.922 | 5.8% | low | 10.0% | 0.02 MiB |
| Archive operations | extract_tar_gz | 274.140 | 281.791 | 10.2% | medium | 11.0% | 0.03 MiB |
| Archive operations | extract_zip | 185.554 | 196.378 | 10.2% | medium | 11.0% | 0.31 MiB |
| Column navigation | load_directory_10000_name_sort | 34.227 | 35.588 | 18.8% | medium | 19.0% | 0.81 MiB |
| Column navigation | load_directory_1000_name_sort | 4.134 | 4.648 | 12.7% | medium | 13.0% | 0.25 MiB |
| Column navigation | load_directory_50000_name_sort | 196.293 | 201.312 | 2.1% | low | 10.0% | 5.02 MiB |
| Column navigation | sort_entries_50000_kind | 7.314 | 7.378 | 1.3% | low | 10.0% | 0.02 MiB |
| Column navigation | sort_entries_50000_modified | 4.378 | 4.755 | 21.2% | medium | 22.0% | 0.03 MiB |
| Column navigation | sort_entries_50000_name | 6.810 | 7.154 | 0.3% | low | 10.0% | 0.02 MiB |
| Column navigation | sort_entries_50000_size | 4.695 | 5.247 | 20.8% | medium | 21.0% | 0.02 MiB |
| Column navigation | sort_entries_50000_vcs | 4.845 | 5.289 | 11.0% | medium | 11.0% | 0.02 MiB |
| Column search | search_directory_10000 | 7.563 | 9.414 | 9.6% | low | 10.0% | 0.09 MiB |
| Column search | search_directory_100000 | 50.352 | 53.776 | 5.5% | low | 10.0% | 0.30 MiB |
| File operations | copy_large_file | 0.945 | 2.072 | 115.1% | high | 116.0% | 0.03 MiB |
| File operations | copy_small_file_tree | 122.360 | 133.636 | 2.7% | low | 10.0% | 0.02 MiB |
| File operations | create_100_files | 6.325 | 15.271 | 148.5% | high | 149.0% | 0.14 MiB |
| File operations | move_small_file_tree | 0.272 | 0.360 | 6.2% | low | 10.0% | 0.05 MiB |
| File operations | rename_file | 0.405 | 1.080 | 6.3% | low | 10.0% | 0.02 MiB |
| Global content search | cold_content_index_build | 4331.986 | 4409.558 | 2.8% | low | 10.0% | 20.42 MiB |
| Global content search | incremental_update_to_query | 180.174 | 184.004 | 4.9% | low | 10.0% | 0.39 MiB |
| Global content search | warm_content_query | 11.378 | 14.879 | 2.8% | low | 10.0% | 0.14 MiB |
| Global name search | global_name_search_10000 | 9.233 | 9.433 | 4.0% | low | 10.0% | 0.03 MiB |
| Global name search | global_name_search_100000 | 89.900 | 92.460 | 2.1% | low | 10.0% | 0.08 MiB |
| Global name search | global_name_search_multi_root | 44.849 | 48.485 | 7.9% | low | 10.0% | 1.73 MiB |
| Preview and editor | text_open_100mib | 0.004 | 0.006 | 22.5% | medium | 23.0% | 0.02 MiB |
| Preview and editor | text_open_1mib | 4.545 | 4.962 | 3.3% | low | 10.0% | 0.02 MiB |
| Preview and editor | text_open_8mib | 31.554 | 31.747 | 0.8% | low | 10.0% | 4.12 MiB |
| Sidebar and devices | volume_enumeration | 23.541 | 25.111 | 1.6% | low | 10.0% | 0.03 MiB |
| Tabs and session restore | load_session_20_tabs | 0.039 | 0.049 | 10.6% | medium | 11.0% | 0.03 MiB |
| Tabs and session restore | save_session_20_tabs | 6.717 | 8.747 | 7.3% | low | 10.0% | 0.05 MiB |
| Update path | sha256_verify_large_file | 772.309 | 775.892 | 0.5% | low | 10.0% | 0.02 MiB |
| VCS and context menus | apply_vcs_statuses | 1.786 | 4.261 | 28.2% | high | 29.0% | 0.94 MiB |
| VCS and context menus | git_clean_status | 121.127 | 127.725 | 84.6% | high | 85.0% | 0.02 MiB |
| VCS and context menus | git_dirty_status_100_files | 122.172 | 126.594 | 83.4% | high | 84.0% | 0.03 MiB |

## High-noise scenarios

| Area | Scenario | p95 spread | Suggested threshold | Recommended action |
| --- | --- | ---: | ---: | --- |
| File operations | copy_large_file | 115.1% | 116.0% | Collect more runs before claiming a regression |
| File operations | create_100_files | 148.5% | 149.0% | Collect more runs before claiming a regression |
| VCS and context menus | apply_vcs_statuses | 28.2% | 29.0% | Collect more runs before claiming a regression |
| VCS and context menus | git_clean_status | 84.6% | 85.0% | Collect more runs before claiming a regression |
| VCS and context menus | git_dirty_status_100_files | 83.4% | 84.0% | Collect more runs before claiming a regression |

## Source reports

- `run-1.json`
- `run-2.json`
- `run-3.json`
