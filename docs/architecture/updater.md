# Kite 更新架构

## 结论

Kite 更新分为四层：

- GitHub Release 发现。
- 平台资产选择。
- SHA-256 校验下载。
- Windows 或 macOS 原子替换。

共享 Rust Core 负责版本检查、资产选择和下载。
平台 UI 负责用户确认、安装启动和重启。
Windows 与 macOS 不共享安装脚本。

## 目标

- 检测 GitHub 最新稳定 Release。
- 比较语义化版本。
- 只接受 Kite 仓库的更新资产。
- 强制校验 SHA-256。
- 支持 Windows EXE 更新。
- 支持 macOS 应用包更新。
- 更新失败时恢复旧版本。
- 不把 GitHub Token 写入客户端。

## 非目标

- 不检查预发布版本。
- 不静默安装更新。
- 不绕过系统权限。
- 不支持未签名第三方更新源。
- 当前不提供增量更新。

## 数据流

```text
用户选择“检查更新”
        |
        v
共享 Rust Core 读取 GitHub Release
        |
        v
比较当前版本与最新版本
        |
        v
选择 Windows EXE 或 macOS ZIP
        |
        v
下载到本地更新目录
        |
        v
校验大小和 SHA-256
        |
        v
平台安装助手替换旧版本
        |
        v
重启 Kite
```

## Release 发现

Kite 使用以下顺序：

1. 读取 `https://api.github.com/repos/wyz541/kite/releases/latest`。
2. API 失败时读取 `https://github.com/wyz541/kite/releases/latest` 的最终跳转地址。
3. 根据跳转标签读取 `SHA256SUMS.txt`。

GitHub API 提供发布说明和资产摘要。
GitHub 页面回退路径规避匿名 API 限流。
回退路径只接受稳定版本标签。
标签必须能解析为语义化版本。

## 版本规则

当前版本来自 `CARGO_PKG_VERSION`。
Release 标签允许带 `v` 前缀。
只有最新版本大于当前版本时，Kite 才提供更新。
相同版本显示“已是最新版本”。
当前版本高于最新版本时不降级。

## 平台资产

### Windows x64

Windows 使用独立资产 `Kite.exe`。
独立 EXE 不依赖 ZIP 解压。
Windows 更新目标为当前运行 EXE。

### macOS arm64

macOS 使用 `Kite-vX.Y.Z-macOS-arm64.zip`。
应用包必须包含 `Kite.app/Contents/Info.plist`。
应用包版本必须来自同一 Release。

## 完整性

Release API 的 `digest` 是首选 SHA-256 来源。
页面回退路径从 `SHA256SUMS.txt` 读取 SHA-256。
摘要格式必须为 64 位十六进制 SHA-256。
资产 URL 必须位于以下前缀：

```text
https://github.com/wyz541/kite/releases/download/
```

下载先写入 `.part` 文件。
Kite 在写入过程中计算 SHA-256。
大小或摘要不匹配时，Kite 删除临时文件。
Kite 只在完整校验后把临时文件改名。

## Windows 安装

Windows Core 创建隐藏 PowerShell 助手。
助手等待当前 Kite PID 退出。
助手等待目标 EXE 文件锁释放。
助手把当前 EXE 移动为 `Kite.exe.old`。
助手复制已经校验的新 EXE。
助手再次计算目标文件 SHA-256。
助手启动新 EXE。
启动成功后，助手删除备份。
任一阶段失败时，助手恢复备份并启动旧 EXE。
日志位于 `%LOCALAPPDATA%\Kite\updates\update.log`。

## macOS 安装

SwiftUI 模型在后台调用共享 Core。
Core 下载并校验 macOS ZIP。
AppKit 层创建 `/bin/bash` 安装助手。
助手等待当前 Kite PID 退出。
助手使用 `/usr/bin/ditto -x -k` 解压应用包。
助手把旧 `Kite.app` 移动为备份。
助手安装新 `Kite.app`。
助手使用 `open` 启动新应用。
启动或解压失败时，助手恢复旧应用。
日志位于 `~/Library/Application Support/Kite/updates/update.log`。

## UI 入口

Windows 入口位于 `Kite > 检查更新…`。
macOS 入口位于应用菜单 `检查更新…`。
Windows 在状态栏显示检查、下载和安装状态。
macOS 在状态栏显示检查、下载和安装状态。
更新可用时，平台 UI 显示版本和 Release 说明。
下载前必须得到用户确认。

## UniFFI

共享 Core 暴露以下记录：

```text
UpdateInfoDto
```

共享 Core 暴露以下方法：

```text
KiteCore::check_for_update()
KiteCore::download_update(update)
```

`UpdateInfoDto` 包含：

- 当前版本。
- 最新版本。
- 可用状态。
- Release 名称和 URL。
- Release 说明。
- 发布时间。
- 平台资产名称和 URL。
- 资产 SHA-256。
- 资产大小。

macOS 不实现第二套版本比较或资产选择逻辑。

## 安全

- 客户端不保存 GitHub Token。
- 更新源固定为 `wyz541/kite`。
- 资产 URL 使用固定前缀校验。
- 摘要缺失时拒绝更新。
- 摘要不匹配时拒绝更新。
- 下载大小限制为 256 MiB。
- Release JSON 限制为 1 MiB。
- 更新不使用动态代码执行。
- PowerShell 脚本参数由进程参数传递。

## 失败处理

- API 限流时使用 Release 页面回退。
- 网络失败时保留当前版本。
- 下载失败时保留当前版本。
- 校验失败时删除临时文件。
- 安装失败时恢复旧版本。
- 权限不足时显示明确错误。
- 更新日志保留失败原因。

## 测试

- 版本解析测试。
- 新版本比较测试。
- 相同版本测试。
- 平台资产选择测试。
- 缺少摘要测试。
- 资产 URL 白名单测试。
- `SHA256SUMS.txt` 解析测试。
- 在线 Release 解析测试。
- 在线 Windows 资产下载和摘要测试。
- Windows 单元测试。
- Windows Clippy。
- Windows 发布构建。
- macOS Core 测试。
- macOS SwiftUI 和 AppKit 构建。

## 后续阶段

1. 增加可配置的自动检查周期。
2. 增加下载进度条。
3. 增加跳过指定版本。
4. 增加更新渠道：稳定版和预览版。
5. 增加 Windows 代码签名验证。
6. 增加 macOS 公证和团队签名验证。
