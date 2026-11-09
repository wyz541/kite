# Kensaku 全局搜索后端

结论：Kite 的全局名称搜索和全局内容搜索完全由 Kensaku 提供。Kite 不再建立自己的全局名称索引或内容索引。

## 1. 进程模型

Kite 启动 `kensaku-daemon`。该进程负责：

1. 首次扫描。
2. 文件监听。
3. 增量更新。
4. 内容提取。
5. 索引持久化。

`kensaku-cli` 只执行只读查询和状态读取。Kite 不直接写入 Kensaku 索引。

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

名称搜索调用：

```text
kensaku-cli search "name:\"<query>\"" --json --limit 5000 --path <root>
```

内容搜索调用：

```text
kensaku-cli search "<query 或 re:\"<pattern>\">" --json --limit <n> --path <root>
```

Kite 对每个根目录执行查询，然后合并、过滤和去重。

## 7. 状态

Kite 通过以下命令读取状态：

```text
kensaku-cli state --json
```

状态用于显示索引文件和内容文档数量。状态缓存 2 秒。

## 8. 错误处理

- CLI 超时：20 秒。
- 状态命令超时：5 秒。
- 输出上限：16 MiB。
- Windows 隐藏控制台窗口。
- Daemon 标准输出和错误写入数据目录日志。
- Kite 不静默回退到本地全盘遍历。

## 9. 兼容范围

- 全局名称搜索：Kensaku。
- 全局内容搜索：Kensaku。
- 分栏内小目录搜索：Kite。
- 文件管理、预览和列导航：Kite。

## 10. 许可证

Kensaku 使用 MIT 许可证。发布包保留上游许可证。
