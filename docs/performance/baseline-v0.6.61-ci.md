# Kite 0.6.61 跨平台 CI 性能基线

结论：Windows x64 和 macOS arm64 的 release 构建都完成自动基准运行。两份结果都来自虚拟 CI 硬件，不能当作实体机绝对性能。

## 1. 来源

- 工作流：[Performance Baseline](https://github.com/wyz541/kite/actions/runs/42337428241)
- Commit：`a921974e09c09326325577d3395b3bf99d7534d2`
- 配置：`ci`
- 构建：`release`
- Windows 原始结果：[JSON](results/v0.6.61-windows-x64-ci.json)
- macOS 原始结果：[JSON](results/v0.6.61-macos-arm64-ci.json)

## 2. 环境

| 字段 | Windows x64 | macOS arm64 |
| --- | --- | --- |
| 系统 | Windows CI | macOS |
| 架构 | x86_64 | aarch64 |
| CPU | Generic x64 CPU | Generic Apple Silicon |
| 逻辑核心 | not recorded | not recorded |
| 内存 | not recorded | not recorded |
| 文件系统 | NTFS | apfs |
| 已测场景 | 34 | 33 |
| 错误 | 0 | 0 |

## 3. 覆盖状态

| 功能域 | Windows | macOS |
| --- | --- | --- |
| Startup and launch | not_automated | not_automated |
| Column navigation | partial | partial |
| Tabs and session restore | partial | partial |
| Directory watch and refresh | partial | not_automated |
| Column search | partial | partial |
| Global name search | measured | measured |
| Global content search | partial | partial |
| Preview and editor | partial | partial |
| File operations | partial | partial |
| Archive operations | partial | partial |
| VCS and context menus | partial | partial |
| Sidebar and devices | partial | partial |
| Update path | partial | partial |

## 4. 结果

延迟单位是毫秒。每个平台分成 p50 和 p95 两列。

| 功能域 | 场景 | Windows p50 | Windows p95 | macOS p50 | macOS p95 |
| --- | --- | ---: | ---: | ---: | ---: |
| Archive operations | `compress_large_file_zip` | 1724.365 | 1816.377 | 1966.388 | 2165.550 |
| Archive operations | `compress_tar_gz` | 20.520 | 20.997 | 17.300 | 22.112 |
| Archive operations | `compress_zip` | 36.386 | 38.758 | 28.380 | 29.862 |
| Archive operations | `extract_tar_gz` | 295.853 | 544.104 | 150.124 | 188.422 |
| Archive operations | `extract_zip` | 240.378 | 288.879 | 135.525 | 175.456 |
| Column navigation | `load_directory_10000_name_sort` | 206.944 | 213.351 | 50.763 | 57.575 |
| Column navigation | `load_directory_1000_name_sort` | 19.840 | 20.475 | 4.485 | 7.504 |
| Column navigation | `sort_entries_10000_kind` | 4.035 | 4.462 | 1.663 | 5.198 |
| Column navigation | `sort_entries_10000_modified` | 3.293 | 3.714 | 0.982 | 9.475 |
| Column navigation | `sort_entries_10000_name` | 3.621 | 3.828 | 1.387 | 2.748 |
| Column navigation | `sort_entries_10000_size` | 3.093 | 3.274 | 0.749 | 1.509 |
| Column navigation | `sort_entries_10000_vcs` | 3.167 | 3.861 | 0.785 | 1.606 |
| Column search | `search_directory_10000` | 10.775 | 12.668 | 18.971 | 23.135 |
| Directory watch and refresh | `windows_notify_create_latency` | 0.340 | 1.215 | - | - |
| File operations | `copy_large_file` | 22.104 | 22.677 | 0.221 | 0.720 |
| File operations | `copy_small_file_tree` | 592.884 | 766.653 | 163.798 | 182.772 |
| File operations | `create_100_files` | 85.419 | 305.632 | 4.660 | 21.230 |
| File operations | `move_small_file_tree` | 1.963 | 12.663 | 0.423 | 0.753 |
| File operations | `rename_file` | 0.325 | 0.339 | 0.168 | 0.305 |
| Global content search | `cold_content_index_build` | 1025.817 | 1076.113 | 4185.175 | 4412.298 |
| Global content search | `incremental_update_to_query` | 75.337 | 79.892 | 100.026 | 115.713 |
| Global content search | `warm_content_query` | 18.872 | 20.626 | 16.161 | 29.749 |
| Global name search | `global_name_search_10000` | 19.316 | 19.629 | 13.660 | 15.888 |
| Global name search | `global_name_search_multi_root` | 40.820 | 41.604 | 24.906 | 48.769 |
| Preview and editor | `text_open_100mib` | 0.057 | 0.063 | 0.009 | 0.011 |
| Preview and editor | `text_open_1mib` | 2.812 | 2.862 | 5.789 | 9.196 |
| Preview and editor | `text_open_8mib` | 19.074 | 19.120 | 49.423 | 57.756 |
| Sidebar and devices | `volume_enumeration` | 0.274 | 0.348 | 14.921 | 61.022 |
| Tabs and session restore | `load_session_20_tabs` | 0.121 | 0.151 | 0.043 | 0.183 |
| Tabs and session restore | `save_session_20_tabs` | 5.394 | 16.124 | 2.386 | 4.795 |
| Update path | `sha256_verify_large_file` | 50.572 | 50.917 | 285.871 | 314.067 |
| VCS and context menus | `apply_vcs_statuses` | 1.890 | 2.764 | 0.943 | 2.331 |
| VCS and context menus | `git_clean_status` | 98.298 | 101.727 | 45.525 | 91.252 |
| VCS and context menus | `git_dirty_status_100_files` | 97.905 | 101.151 | 36.061 | 67.126 |

## 5. 主要发现

- Windows 最大 p95：`Archive operations / compress_large_file_zip`，1816.377 ms。
- Windows 最大抖动：`File operations / move_small_file_tree`，p95/p50 为 6.45。
- Windows 最大 RSS 增量：`Preview and editor / text_open_8mib`，16.04 MiB。
- macOS 最大 p95：`Global content search / cold_content_index_build`，4412.298 ms。
- macOS 最大抖动：`Column navigation / sort_entries_10000_modified`，p95/p50 为 9.65。
- macOS 最大 RSS 增量：`Global content search / cold_content_index_build`，14.52 MiB。

## 6. 解释

- 两个平台使用不同 CPU、核心数、内存和操作系统。表内并排数据只用于各自基线，不用于平台胜负判断。
- 当前配置是 `ci`，搜索树上限为 10K。`standard` 配置使用 50K 目录和 100K 搜索树。
- Windows 包含目录监听创建延迟。macOS 使用 Swift DispatchSource，仍需原生 runner。
- Windows 和 macOS 的启动、原生 UI 渲染、WPD/MTP、USB、网络和安装器测量仍为缺口。
- 所有缺口保留在各平台的 `coverage` 和 JSON `results` 中。
