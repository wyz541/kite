# Kite performance comparison

- Baseline: `0.6.62` / `e7c8b47cdc149d997e08f4ef4ee9d44f7dfa5976` / `standard`
- Candidate: `0.6.62` / `e7c8b47cdc149d997e08f4ef4ee9d44f7dfa5976` / `standard`
- Global threshold: `10.0%` on p95
- Scenario noise thresholds: `enabled`

> The optimized report was generated from the uncommitted patch. The patch commit is `7eda754657f65aa1041d10fea726885992f9145d`.

| Area | Scenario | Boundary | Baseline p95 ms | Candidate p95 ms | p95 delta | Threshold | p50 delta | n | Result |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Global content search | cold_content_index_build | core | 4164.647 | 5156.005 | +23.8% | 10.0% | +24.3% | 10/10 | above_threshold |
| Tabs and session restore | save_session_20_tabs | core | 7.993 | 9.650 | +20.7% | 17.0% | -4.2% | 30/30 | above_threshold |
| File operations | move_small_file_tree | core | 0.381 | 0.453 | +19.0% | 22.0% | +18.6% | 10/10 | stable |
| File operations | copy_large_file | core | 1.784 | 2.089 | +17.1% | 24.0% | -17.7% | 10/10 | stable |
| Tabs and session restore | load_session_20_tabs | core | 0.050 | 0.058 | +15.7% | 43.0% | +13.7% | 30/30 | stable |
| Column search | search_directory_100000 | core | 55.322 | 59.411 | +7.4% | 25.0% | +4.1% | 30/30 | stable |
| Global name search | global_name_search_10000 | core | 9.823 | 10.260 | +4.4% | 11.0% | +2.5% | 30/30 | stable |
| Sidebar and devices | volume_enumeration | core | 50.003 | 51.854 | +3.7% | 10.0% | +1.8% | 30/30 | stable |
| Column search | search_directory_10000 | core | 9.347 | 9.573 | +2.4% | 10.0% | +6.3% | 30/30 | stable |
| VCS and context menus | git_clean_status | core | 45.188 | 46.079 | +2.0% | 55.0% | +10.8% | 10/10 | stable |
| Preview and editor | text_open_1mib | core | 4.580 | 4.627 | +1.0% | 10.0% | -0.8% | 10/10 | stable |
| Global name search | global_name_search_100000 | core | 102.424 | 103.187 | +0.7% | 10.0% | +1.4% | 30/30 | stable |
| Archive operations | compress_large_file_zip | core | 5132.752 | 5162.019 | +0.6% | 10.0% | +0.6% | 5/5 | stable |
| Preview and editor | text_open_8mib | core | 31.449 | 31.622 | +0.6% | 10.0% | +1.3% | 10/10 | stable |
| Column navigation | sort_entries_50000_name | core | 6.632 | 6.657 | +0.4% | 10.0% | -0.6% | 30/30 | stable |
| File operations | rename_file | core | 1.497 | 1.494 | -0.1% | 41.0% | -53.4% | 30/30 | stable |
| Update path | sha256_verify_large_file | core | 780.502 | 778.003 | -0.3% | 10.0% | +0.3% | 5/5 | stable |
| Global name search | global_name_search_multi_root | core | 49.374 | 49.087 | -0.6% | 10.0% | +1.4% | 30/30 | stable |
| Column navigation | sort_entries_50000_kind | core | 6.936 | 6.882 | -0.8% | 10.0% | -1.8% | 30/30 | stable |
| Archive operations | compress_zip | core | 108.095 | 107.196 | -0.8% | 35.0% | +2.5% | 10/10 | stable |
| Archive operations | extract_tar_gz | core | 290.066 | 287.355 | -0.9% | 10.0% | -1.7% | 10/10 | stable |
| File operations | create_100_files | core | 33.880 | 33.388 | -1.5% | 67.0% | -4.2% | 30/30 | stable |
| File operations | copy_small_file_tree | core | 141.279 | 139.215 | -1.5% | 10.0% | -1.3% | 30/30 | stable |
| Global content search | warm_content_query | core | 15.344 | 14.879 | -3.0% | 16.0% | +1.2% | 30/30 | stable |
| Archive operations | extract_zip | core | 199.824 | 192.919 | -3.5% | 10.0% | -1.3% | 10/10 | stable |
| Column navigation | sort_entries_50000_modified | core | 4.970 | 4.783 | -3.8% | 10.0% | -10.0% | 30/30 | stable |
| Column navigation | sort_entries_50000_vcs | core | 4.684 | 4.451 | -5.0% | 10.0% | -7.7% | 30/30 | stable |
| Archive operations | compress_tar_gz | core | 53.994 | 51.255 | -5.1% | 16.0% | +1.4% | 10/10 | stable |
| Preview and editor | text_open_100mib | core | 0.006 | 0.005 | -5.2% | 10.0% | +12.0% | 10/10 | stable |
| VCS and context menus | git_dirty_status_100_files | core | 45.417 | 42.966 | -5.4% | 65.0% | -7.7% | 10/10 | stable |
| Column navigation | sort_entries_50000_size | core | 4.653 | 4.329 | -6.9% | 10.0% | +0.4% | 30/30 | stable |
| Global content search | incremental_update_to_query | core | 203.188 | 188.454 | -7.3% | 10.0% | +2.2% | 10/10 | stable |
| VCS and context menus | apply_vcs_statuses | core | 4.421 | 3.762 | -14.9% | 19.0% | -16.5% | 30/30 | stable |
| Column navigation | load_directory_1000_name_sort | core | 4.617 | 3.871 | -16.2% | 10.0% | -22.3% | 30/30 | below_threshold |
| Column navigation | load_directory_10000_name_sort | core | 38.107 | 31.185 | -18.2% | 15.0% | -15.0% | 30/30 | below_threshold |
| Column navigation | load_directory_50000_name_sort | core | 220.473 | 178.037 | -19.2% | 10.0% | -18.1% | 30/30 | below_threshold |
