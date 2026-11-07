# Kite 0.6.61 跨平台启动基线

结论：Windows x64 和 macOS arm64 的 release 构建都完成真实进程启动。首内容事件已验证。

## 1. 来源

- 工作流：[Startup Performance](https://github.com/wyz541/kite/actions/runs/42347348032)
- 版本：`0.6.61`
- 配置：3 次进程启动，10K 文件夹具
- Windows 原始结果：[JSON](results/v0.6.61-windows-x64-startup-ci.json)
- macOS 原始结果：[JSON](results/v0.6.61-macos-arm64-startup-ci.json)

## 2. 环境

| 字段 | Windows x64 | macOS arm64 |
| --- | --- | --- |
| 系统 | Windows CI | macOS CI |
| 架构 | AMD64 | arm64 |
| CPU | Generic x64 CPU | Generic Apple Silicon |
| 内存 | not recorded | not recorded |
| 夹具文件 | 10000 | 10000 |

## 3. 结果

| 平台 | n | 启动到首内容 p50 ms | p95 ms | max ms | 首进程 ms | 热重启 p50 ms | 热点 p95 ms | 峰值 RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Windows x64 | 3 | 558.000 | 828.900 | 859.000 | 859.000 | 526.000 | 554.800 | 63.70 |
| macOS arm64 | 3 | 2009.000 | 8616.800 | 9351.000 | 9351.000 | 1893.000 | 1997.400 | 139.69 |

## 4. 解释

- 两端都使用 release 构建和 10K 文件目录。
- macOS 首次 CI 进程需要约 9.35 秒。后续两个热重启约 1.8 到 2.0 秒。
- Windows 首次 CI 进程约 0.86 秒。后续两个热重启约 0.49 到 0.56 秒。
- 首次 CI 进程包含虚拟机和冷缓存影响。不要把单次值当作实体机冷启动保证。
- 本任务的 macOS 实体机 10 次基线是 [startup-macos-v0.6.61.md](startup-macos-v0.6.61.md)。
- 首内容事件是 SwiftUI/GPUI 内容事件，不是合成器确认的首帧。
- 原生交互丢帧仍需单独测量。
