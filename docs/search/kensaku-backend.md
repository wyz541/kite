# Kensaku 全局搜索后端

结论：Kite 的全局名称搜索和全局内容搜索完全由 Kensaku 提供。Kite 不再建立自己的全局名称索引或内容索引。

## 1. 进程模型

Kite 启动 `kensaku-daemon`。该进程负责：

1. 首次扫描。
2. 文件监听。
3. 增量更新。
4. 内容提取。
5. 索引持久化。

Kite 通过本地 JSON-RPC 连接 daemon，读取实时状态并执行实时查询。
`kensaku-cli` 只在 daemon 连接不可用时读取持久化索引。
Kite 不直接写入 Kensaku 索引。

## 2. 数据目录

Kite 使用独立数据目录：

```text
<Kite local data>/kensaku
```

Windows 默认位置示例：

```text
%LOCALAPPDATA%\Kite\kensaku
```

可用 `KITE_KENSAKU_DATA_DIR` 覆盖。

Kite 在数据目录写入 `config.json`。Kite 设置以下字段：

```json
{
  "watched_dirs": ["C:\\Users\\name"],
  "is_setup_complete": true,
  "auto_update": false
}
```

其他 Kensaku 配置使用上游默认值。

## 3. Sidecar 发现顺序

CLI：

1. `KITE_KENSAKU_CLI`
2. Kite 可执行文件旁的 `kensaku-cli.exe` 或 `kensaku-cli`
3. `PATH`

Daemon：

1. `KITE_KENSAKU_DAEMON`
2. Kite 可执行文件旁的 `kensaku-daemon.exe` 或 `kensaku-daemon`
3. `PATH`

Daemon 发现只检查文件存在。
Kite 不执行 `kensaku-daemon --help`，因为 daemon 会进入服务模式。

Windows `v0.6.65` 及更高版本发现 sidecar 缺失时，Kite 自动下载当前稳定 Release ZIP。
Kite 校验 ZIP 的 SHA-256 后安装 `kensaku-cli.exe`、`kensaku-daemon.exe` 和 `onnxruntime.dll`。

## 4. 构建 Sidecar

运行：

```powershell
python scripts/kensaku/build_sidecars.py
```

脚本执行以下动作：

1. 下载固定版本 `v1.0.16` 的 Kensaku CLI。
2. 校验 CLI SHA-256。
3. 下载固定提交 `9197751` 的 Kensaku 源码。
4. 构建 `kensaku-daemon`。
5. 在 Windows daemon 中嵌入 Common Controls v6 manifest。
6. 下载并校验 ONNX Runtime 1.31.0。
7. 把 sidecar 和运行时写入 `vendor/kensaku`。

Windows 发布构建默认执行该脚本。使用 `-SkipKensaku` 可跳过 sidecar 构建。

macOS 发布构建默认执行该脚本。设置 `KITE_SKIP_KENSAKU=1` 可跳过 sidecar 构建。

## 5. 根目录更新

Kite 把新的搜索根目录加入 `watched_dirs`。

已有 daemon 的配置不覆盖新根目录时，Kite 执行以下步骤：

1. 停止 Kite 管理的旧 daemon。
2. 写入合并后的根目录。
3. 重启 daemon。

Kite 只增加根目录。Kite 不删除旧根目录，以免意外删除用户索引。

## 6. 查询

Kite 对每个根目录调用 daemon `search` 方法。

名称搜索请求等价于：

```text
name:"<query>" in:"<root>"
```

内容搜索请求等价于：

```text
<query 或 re:"<pattern>"> in:"<root>"
```

Daemon 在扫描期间返回当前内存中的结果。
Kite 合并、过滤和去重结果。
索引期间 Kite 每 2 秒重新查询。

Daemon 连接失败时，Kite 使用等价 CLI 参数查询持久化索引：

```text
kensaku-cli search "<query>" --json --limit <n> --path <root>
```

## 7. 状态

首选 daemon 方法：

```text
index_state
```

状态包含 `is_indexing`、`phase`、`files_indexed`、`extract_done` 和 `extract_total`。
状态用于显示扫描位置、索引数量和内容提取进度。
Kite 缓存状态 2 秒。

Daemon 连接失败时，Kite 回退到：

```text
kensaku-cli state --json
```

## 8. 错误处理

- daemon IPC 优先。
- CLI 回退搜索超时：20 秒。
- CLI 回退状态超时：5 秒。
- 输出上限：16 MiB。
- Windows 隐藏控制台窗口。
- Daemon 标准输出和错误写入数据目录日志。
- Kite 不静默回退到本地全盘遍历。
- 首次目录遍历完成前，实时名称结果可能为 0 项。
- Kite 在索引期间自动刷新当前查询。

## 9. 兼容范围

- 全局名称搜索：Kensaku。
- 全局内容搜索：Kensaku。
- 分栏内小目录搜索：Kite。
- 文件管理、预览和列导航：Kite。

## 10. 许可证

Kensaku 使用 MIT 许可证。发布包保留上游许可证。
