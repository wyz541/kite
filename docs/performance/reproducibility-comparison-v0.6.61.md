# Kite performance comparison

- Baseline: `0.6.61` / `ae82ed2f682f861a2f50704bf1f62e91be437274` / `standard`
- Candidate: `0.6.61` / `ae82ed2f682f861a2f50704bf1f62e91be437274` / `standard`
- Regression threshold: `20.0%` on p95

| Area | Scenario | Boundary | Baseline p95 ms | Candidate p95 ms | p95 delta | p50 delta | n | Result |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| VCS and context menus | git_dirty_status_100_files | core | 67.973 | 110.965 | +63.2% | +61.4% | 10/10 | above_threshold |
| VCS and context menus | git_clean_status | core | 71.081 | 107.199 | +50.8% | +59.2% | 10/10 | above_threshold |
| File operations | create_100_files | core | 23.175 | 28.037 | +21.0% | +1.9% | 30/30 | above_threshold |
| Archive operations | extract_tar_gz | core | 324.586 | 389.820 | +20.1% | +4.5% | 10/10 | above_threshold |
| Archive operations | extract_zip | core | 202.628 | 221.905 | +9.5% | +5.3% | 10/10 | stable |
| Global name search | global_name_search_multi_root | core | 48.912 | 53.272 | +8.9% | +0.4% | 30/30 | stable |
| Archive operations | compress_tar_gz | core | 55.604 | 57.227 | +2.9% | +0.8% | 10/10 | stable |
| Column navigation | load_directory_10000_name_sort | core | 41.894 | 42.808 | +2.2% | +0.3% | 30/30 | stable |
| Preview and editor | text_open_1mib | core | 4.029 | 4.114 | +2.1% | +2.3% | 10/10 | stable |
| Global name search | global_name_search_10000 | core | 10.124 | 10.306 | +1.8% | -1.6% | 30/30 | stable |
| Global name search | global_name_search_100000 | core | 100.043 | 101.723 | +1.7% | +0.2% | 30/30 | stable |
| Column navigation | sort_entries_50000_name | core | 6.194 | 6.257 | +1.0% | +0.1% | 30/30 | stable |
| Archive operations | compress_large_file_zip | core | 5509.136 | 5552.445 | +0.8% | +2.1% | 5/5 | stable |
| Global content search | warm_content_query | core | 12.153 | 12.247 | +0.8% | +0.6% | 30/30 | stable |
| Column navigation | sort_entries_50000_size | core | 3.431 | 3.452 | +0.6% | -1.6% | 30/30 | stable |
| Preview and editor | text_open_100mib | core | 0.008 | 0.008 | +0.4% | -0.9% | 10/10 | stable |
| Column navigation | sort_entries_50000_kind | core | 6.402 | 6.420 | +0.3% | +0.0% | 30/30 | stable |
| Preview and editor | text_open_8mib | core | 32.493 | 32.577 | +0.3% | +0.2% | 10/10 | stable |
| Update path | sha256_verify_large_file | core | 817.326 | 818.759 | +0.2% | +0.1% | 5/5 | stable |
| Column navigation | sort_entries_50000_modified | core | 3.783 | 3.755 | -0.7% | +0.0% | 30/30 | stable |
| Archive operations | compress_zip | core | 111.221 | 109.811 | -1.3% | +0.7% | 10/10 | stable |
| Global content search | incremental_update_to_query | core | 202.207 | 199.577 | -1.3% | -0.5% | 10/10 | stable |
| Global content search | cold_content_index_build | core | 3554.824 | 3507.070 | -1.3% | -1.6% | 10/10 | stable |
| Column navigation | sort_entries_50000_vcs | core | 3.489 | 3.432 | -1.6% | -1.1% | 30/30 | stable |
| VCS and context menus | apply_vcs_statuses | core | 1.765 | 1.724 | -2.3% | -1.1% | 30/30 | stable |
| File operations | copy_small_file_tree | core | 143.121 | 139.176 | -2.8% | -0.8% | 30/30 | stable |
| Column navigation | load_directory_50000_name_sort | core | 224.059 | 216.934 | -3.2% | -3.3% | 30/30 | stable |
| Tabs and session restore | save_session_20_tabs | core | 5.907 | 5.717 | -3.2% | -7.5% | 30/30 | stable |
| Column search | search_directory_100000 | core | 63.782 | 61.146 | -4.1% | -8.0% | 30/30 | stable |
| File operations | rename_file | core | 0.163 | 0.153 | -6.3% | -7.9% | 30/30 | stable |
| Column search | search_directory_10000 | core | 8.856 | 8.151 | -8.0% | -1.1% | 30/30 | stable |
| Column navigation | load_directory_1000_name_sort | core | 3.864 | 3.486 | -9.8% | -2.7% | 30/30 | stable |
| File operations | copy_large_file | core | 0.385 | 0.322 | -16.4% | -24.3% | 10/10 | stable |
| File operations | move_small_file_tree | core | 0.446 | 0.349 | -21.7% | -23.2% | 10/10 | below_threshold |
| Tabs and session restore | load_session_20_tabs | core | 0.047 | 0.037 | -21.8% | -3.3% | 30/30 | below_threshold |
| Sidebar and devices | volume_enumeration | core | 84.666 | 28.513 | -66.3% | -53.4% | 30/30 | below_threshold |
