# Kensaku 搜索后端

结论：Kite 检测到 Kensaku CLI 和可用的 Kensaku 索引后，名称搜索和内容搜索优先使用 Kensaku。

## 1. 适用范围

- 全局名称搜索
- 全局内容搜索
- 当前文件夹、收藏夹和本地磁盘范围

分栏内的小目录搜索继续使用 Kite Core。小目录搜索不建立跨进程索引。

## 2. 后端发现顺序

Kite 按以下顺序寻找 `kensaku-cli`：

1. 环境变量 `KITE_KENSAKU_CLI` 指定的路径。
2. Kite 可执行文件旁边的 `kensaku-cli.exe` 或 `kensaku-cli`。
3. `PATH` 中的 `kensaku-cli`。

Kensaku 固定测试版本：`v1.0.16`。上游固定提交：`9197751`。

## 3. 调用协议

Kite 使用 Kensaku CLI 的一次性只读命令。

名称搜索：

```text
kensaku-cli search "name:\"<query>\"" --json --limit 5000 --path <root>
```

内容搜索：

```text
kensaku-cli search "<query 或 re:\"<pattern>\">" --json --limit <n> --path <root>
```

Kite 对每个根目录单独执行命令。Kite 合并结果并执行最终去重。

## 4. 覆盖判断

Kite 先执行：

```text
kensaku-cli state --json
```

只有 Kensaku 索引根目录覆盖全部请求根目录时，Kensaku 快照才标记为完整。

未覆盖时，Kite 保留本地名称索引、Spotlight、Everything 和遍历回退。

## 5. 数据映射

名称结果映射到 `FileEntry`。

内容结果映射到 `ContentFileMatch`。Kensaku 的搜索结果提供片段，不提供完整行位置。Kite 在文件中查找该片段，并尝试恢复行号。

该映射是兼容层。后续应通过 Kensaku MCP `read_snippet` 获取权威行号和上下文。

## 6. 回退顺序

名称搜索回退顺序：

1. Kensaku
2. Kite 名称索引
3. Spotlight 或 Everything
4. 文件系统遍历

内容搜索回退顺序：

1. Kensaku
2. Kite 内容索引
3. 受控实时读取

## 7. 限制

- Kensaku CLI 只读取已有 Kensaku 索引。
- Windows 没有 Kensaku daemon 时，Kensaku CLI 不自建索引。
- 用户需要先运行 Kensaku，或由发布包启动 Kensaku 索引服务。
- Kensaku 索引停止监听后，结果可能落后于磁盘。
- Kite 不修改 Kensaku 索引。
- 内容行号是兼容映射结果，不保证等同 Kensaku 原生上下文。

## 8. 安全边界

Kite 不通过 shell 拼接命令。Kite 使用参数数组调用 CLI。

Kite 为命令设置 20 秒上限。Kite 为输出设置 16 MiB 上限。Kite 在 Windows 隐藏控制台窗口。

## 9. 许可证

Kensaku 使用 MIT 许可证。发布包附带 Kensaku CLI 时需要保留上游许可证和版本信息。
