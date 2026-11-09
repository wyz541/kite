# Kite 性能基线

结论：`kite-perf` 为 Rust Core 的主要性能路径生成可复现 JSON 报告。原生 UI、网络和受控设备仍使用独立边界，不混入 Core 数值。

当前实体机基线：[v0.6.61 macOS arm64](baseline-v0.6.61.md)。
当前跨平台 CI 基线：[v0.6.61 Windows 与 macOS](baseline-v0.6.61-ci.md)。
当前三次运行聚合基线：[v0.6.61 macOS arm64](aggregate-v0.6.61.md)。
当前复现检查：[v0.6.61 双次运行](reproducibility-v0.6.61.md)。
当前 macOS 启动基线：[v0.6.61 启动与首内容](startup-macos-v0.6.61.md)。
当前跨平台启动 CI 基线：[v0.6.61 Windows 与 macOS](startup-ci-v0.6.61.md)。

## 1. 目标

`kite-perf` 回答三个问题。

1. 当前版本的主要 Core 路径有多快。
2. 新版本相对基线是否变慢。
3. 哪些结果还不能自动测量，原因是什么。

性能报告比较同一平台、同一配置和同一数据规模。Kite 不生成跨平台总分。

## 2. 快速运行

运行短基准：

```bash
cargo run --locked --release --features performance --bin kite-perf -- \
  --profile smoke \
  --samples 5 \
  --cold-samples 3 \
  --output target/performance/smoke.json \
  --markdown target/performance/smoke.md
```

运行发布基线：

```bash
cargo run --locked --release --features performance --bin kite-perf -- \
  --profile standard \
  --output docs/performance/results/v0.6.61-macos-arm64.json \
  --markdown docs/performance/baseline-v0.6.61.md
```

运行扩展压力基线：

```bash
cargo run --locked --release --features performance --bin kite-perf -- \
  --profile extended \
  --keep-fixtures \
  --output target/performance/extended.json \
  --markdown target/performance/extended.md
```

`--keep-fixtures` 保留临时数据。默认运行结束后删除临时数据。

## 3. 配置档

| 配置档 | 目录规模 | 搜索树规模 | 交互样本 | 冷样本 | 用途 |
| --- | --- | --- | ---: | ---: | --- |
| `smoke` | 1K | 5K | 5 | 3 | 本地快速检查 |
| `ci` | 1K、10K | 10K | 10 | 5 | 跨平台 CI |
| `standard` | 1K、10K、50K | 10K、100K | 30 | 10 | 发布基线 |
| `extended` | 1K、10K、50K | 10K、100K、250K | 50 | 20 | 压力边界 |

`--samples` 和 `--cold-samples` 可以覆盖样本数。

## 4. 指标约定

- 延迟单位是毫秒。
- p50 和 p95 使用排序样本的线性插值。
- 每个延迟结果记录 p50、p95、max、均值和样本数。
- RSS 是进程常驻内存。采样线程每 10 ms 读取一次，再与操作结束后的 RSS 取最大值。
- CPU 时间使用进程累计 CPU 毫秒。
- I/O 使用进程累计读写字节的差值。
- `items/s` 和 `MiB/s` 是吞吐量。慢操作不能只用吞吐量解释。
- 原始样本保留在 JSON 中。
- 缓存不主动清理。报告明确此项限制。

## 5. 自动覆盖

| 功能域 | 自动测量 | 当前缺口 |
| --- | --- | --- |
| 启动与路径启动 | 进程启动到首个稳定内容事件，含 RSS | 合成器确认的首帧、默认处理程序入口 |
| 分栏导航 | 目录读取、五种排序 | 分栏渲染、筛选、分组、丢帧 |
| 页签与会话 | 20 页签保存和恢复 | 页签打开、切换、关闭和渲染 |
| 目录监听 | Windows 监听创建延迟 | macOS 监听、刷新应用、丢帧 |
| 分栏搜索 | 递归名称搜索 | 搜索框反馈和丢帧 |
| 全局名称搜索 | Kensaku sidecar 查询转换与过滤 | sidecar 打包后需增加冷建、热查询和增量更新基准 |
| 全局内容搜索 | Kensaku sidecar 查询转换 | sidecar 打包后需增加内容索引和 OCR 基准 |
| 预览和编辑 | 1 MiB、8 MiB 文本读取；100 MiB 拒绝 | 图片、Markdown、代码、滚动和大日志 |
| 文件操作 | 创建、重命名、复制、移动、大文件复制 | 回收站、剪贴板、进度和丢帧 |
| 压缩 | ZIP 和 TAR.GZ 压缩、解压 | 原生进度和丢帧 |
| VCS 与菜单 | Git 状态和状态应用 | SVN 环境、Windows Shell 菜单 |
| 侧边栏与设备 | 卷枚举 | WPD、MTP、USB、网络卷 |
| 更新 | SHA-256 校验 | GitHub API、下载、安装和重启 |

缺口会写入 JSON 的 `coverage` 和 `results`。缺口不等于通过。


## 6. 原生 UI 手工流程

原生 UI 数据必须由真实 release 应用产生。记录以下字段：

| 字段 | 内容 |
| --- | --- |
| 日期 | ISO 8601 |
| 设备 | 机型或虚拟机规格 |
| 系统 | 平台版本 |
| 构建 | 版本、commit、release 包 |
| 场景 | 启动、分栏、搜索、预览或操作 |
| 数据 | 固定测试数据名称 |
| 样本 | 至少 30 个交互样本，至少 10 个冷启动样本 |
| 起点 | 用户动作或进程启动 |
| 终点 | 第一个稳定可见结果 |
| UI 卡顿 | 主线程卡顿或丢帧数据 |

启动测量流程：

1. 重启应用进程。保持系统缓存状态不变。
2. 从同一启动入口打开同一测试目录。
3. 记录进程启动到首个稳定目录列表可见的时间。
4. 重复至少 10 次。
5. 记录 p50、p95、max 和峰值 RSS。

交互测量流程：

1. 使用 `standard` 配置生成数据。
2. 先执行一次不计时预热。
3. 对同一动作执行至少 30 次。
4. 从输入事件开始计时。
5. 到稳定可见结果停止计时。
6. 记录 p50、p95、max、丢帧和峰值 RSS。

两个平台必须使用相同动作、相同数据规模和相同停止条件。

## 7. 启动测量

启动测量器使用 release 应用和新进程。macOS 事件是首批目录加载后的 `KiteColumnBrowser.onAppear`。Windows 事件是首批目录加载后的首次 `KiteApp` 渲染。

macOS：

```bash
bash apps/macos/build.sh
python3 scripts/performance/startup.py   --app apps/macos/build/Kite.app   --samples 10   --entries 10000   --output docs/performance/results/v0.6.61-macos-arm64-startup.json   --markdown docs/performance/startup-macos-v0.6.61.md
```

Windows：

```powershell
cargo build --release --locked --features windows-ui
python scripts/performance/startup.py `
  --app target/release/kite-explorer.exe `
  --samples 10 `
  --entries 10000 `
  --output target/performance/startup-windows.json `
  --markdown target/performance/startup-windows.md
```

工作流 `.github/workflows/performance-startup.yml` 在两个平台构建 release 应用并上传启动结果。相关 PR 路径会触发 3 次快速测量。手工运行可以增加样本数和夹具规模。

测量器记录：

- 从进程创建到首个内容事件的时间。
- 从应用入口到首个内容事件的时间。
- 每个样本的峰值 RSS。
- 全部追踪事件和原始样本。

边界：

- 首内容事件是可见内容代理。它不是合成器确认的首帧。
- 测量器不清理系统缓存。`first_process` 只表示首次进程；`warm_restart` 使用热缓存。
- 原生交互丢帧、默认文件管理器入口和 Finder 入口仍需独立测量。

## 8. 跨平台工作流

工作流文件是 `.github/workflows/performance.yml`。

- Pull Request 使用 `ci` 配置和 1 次独立运行。
- 手工运行可以选择 `smoke`、`ci`、`standard` 或 `extended`，默认运行 3 次。
- 两个平台上传每次原始 JSON、每次 Markdown 和聚合结果。
- 启动工作流对相关 PR 路径运行 3 次。手工运行可以使用更多样本。
- 工作流不阻断合并。比较时使用聚合结果和场景噪声阈值。

## 9. 三次运行聚合

发布比较使用三个独立进程运行。每个运行使用新的临时夹具。

```bash
python3 scripts/performance/aggregate.py \
  run-1.json \
  run-2.json \
  run-3.json \
  --output aggregate.json \
  --markdown aggregate.md
```

聚合规则：

- 每个场景使用三次运行 p50 的中位数。
- 每个场景使用三次运行 p95 的中位数。
- p95 抖动 = `(最大 p95 - 最小 p95) / 中位数 p95`。
- 抖动不高于 10% 时标记 `low`。
- 抖动高于 10% 且不高于 25% 时标记 `medium`。
- 抖动高于 25% 时标记 `high`。
- 场景比较阈值取 10% 与该场景 p95 抖动中的较大值。

`v0.6.61` 聚合结果：

| 噪声等级 | 场景数 |
| --- | ---: |
| `low` | 22 |
| `medium` | 9 |
| `high` | 5 |

高噪声场景包括大文件复制、100 文件创建和 Git 状态。发布判断前必须增加运行次数。

## 10. 版本比较

比较两个 JSON 报告：

```bash
python3 scripts/performance/compare.py \
  aggregate.json \
  candidate-aggregate.json \
  --threshold 10 \
  --use-baseline-noise \
  --markdown comparison.md
```

`--use-baseline-noise` 使用聚合报告中的场景阈值。稳定场景通常使用 10%。高噪声场景使用实测抖动作为阈值。

脚本把超过阈值的 p95 变化标记为 `above_threshold`。相同构建重复运行也可能出现该标记。脚本不自动证明原因。比较结果只用于定位需要复核的场景。

需要让脚本在发现回归时返回失败时，增加 `--fail-on-regression`。

## 11. 发布候选流程

1. 构建 release 版本。
2. 在 Windows x64 和 macOS arm64 各运行三次独立 `standard` 配置。
3. 保存每次原始 JSON，并生成聚合 JSON 和 Markdown。
4. 检查系统、CPU、文件系统、commit 和样本数是否相同。
5. 使用聚合报告运行噪声感知比较。
6. 对超过场景阈值的项目增加运行次数。
7. 将重复出现的退化拆成独立 Issue。
8. 将聚合报告、比较结果和结论写入性能追踪 Issue。

## 12. 解释边界

- 时间来自单一进程。系统负载会影响结果。
- 操作系统缓存没有统一清理。读取类结果通常偏热。
- APFS 可能使用写时复制。大文件复制吞吐量可能高于实际数据重写速度。
- 小文件操作受文件系统和杀毒软件影响。
- 网络结果只用于环境记录，不作为固定应用目标。
- 移动设备吞吐量取决于设备、线缆、驱动和 Shell。
