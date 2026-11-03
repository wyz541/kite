<p align="center">
  <img src="assets/kite-icon-master.png" width="132" alt="Kite icon">
</p>

<h1 align="center">Kite</h1>

<p align="center">
  <strong>为 Windows 与 macOS 打造的原生分栏文件管理器</strong>
</p>

<p align="center">
  在一处查看目录层级、文件内容、预览和操作，减少窗口切换与路径迷路。
</p>

<p align="center">
  <a href="https://github.com/wyz541/kite/releases/latest"><img src="https://img.shields.io/github/v/release/wyz541/kite?style=flat-square&label=release" alt="Latest release"></a>
  <a href="https://github.com/wyz541/kite/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/wyz541/kite/ci.yml?branch=main&style=flat-square&label=CI" alt="CI status"></a>
  <img src="https://img.shields.io/badge/Windows-x64-0078D4?style=flat-square&logo=windows11&logoColor=white" alt="Windows x64">
  <img src="https://img.shields.io/badge/macOS-arm64-000000?style=flat-square&logo=apple&logoColor=white" alt="macOS arm64">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-2F81F7?style=flat-square" alt="MIT License"></a>
</p>

<p align="center">
  <a href="https://github.com/wyz541/kite/releases/latest"><strong>下载最新版本</strong></a>
  ·
  <a href="docs/architecture/platforms.md">平台说明</a>
  ·
  <a href="docs/design/manifesto.md">设计原则</a>
</p>

---

## Kite 是什么

Kite 是一款本地优先的原生文件管理器。
它以分栏导航为核心，把目录层级、文件列表、内容预览、属性信息和常用操作放在同一个工作区。

Kite 适合需要频繁移动文件的开发者、设计人员和经常整理大量文件的桌面用户。
它不要求用户记住完整路径，也不要求用户通过多个窗口维持上下文。

> 看见路径，理解位置，然后直接行动。

## 下载

请从 [GitHub Releases](https://github.com/wyz541/kite/releases/latest) 下载最新版本。

| 平台 | 发布包 | 系统要求 |
| --- | --- | --- |
| Windows x64 | `Kite-vX.Y.Z-windows-x64.zip` 或 `Kite.exe` | Windows 10 / 11 |
| macOS Apple Silicon | `Kite-vX.Y.Z-macOS-arm64.zip` | macOS 13 或更新版本 |

macOS 26 及更新版本使用 Liquid Glass 导航外壳。
macOS 13 到 15 自动使用标准材质。

## 为什么选择 Kite

### 分栏让你保留上下文

选中文件夹后，下一级目录出现在右侧。
上级目录继续可见，当前路径和选择关系保持清晰。
窗口变窄时，栏目进入横向滚动区域，不丢失已加载的层级。

### 每个平台都使用原生界面

Windows 使用 Rust、GPUI 和 Windows Shell。
macOS 使用 SwiftUI、AppKit、Quick Look 和 Finder 集成。
两端共享 Rust Core，但各自保留平台原生交互。

### 文件和操作放在一起

Kite 支持复制、剪切、粘贴、重命名、新建、回收站和“拷贝到…”操作。
Windows 可与 Explorer 共享文件剪贴板。
macOS 可与 Finder 共享文件剪贴板。

### 工作中需要的上下文直接可见

在文件列表旁查看属性、图片、纯文本、Markdown 和代码预览。
在 Git 或 SVN 工作区查看状态徽标并执行常用操作。
详细模式显示日期、类型和大小。

## 功能

### 导航

- 多栏文件夹导航，栏数由窗口宽度决定。
- 页签、返回、前进、上一级和可编辑面包屑。
- 侧边栏快速访问、收藏、设备和磁盘入口。
- 会话恢复，包括页签、路径和视图模式。
- 分栏标题栏排序方向切换。
- Windows 支持按名称、修改日期、类型和大小排序。
- Windows 支持分栏筛选、分组和显示规则重置。

### 文件管理

- 新建文件、新建文件夹、重命名和移到回收站。
- 复制、剪切和粘贴文件或文件夹。
- 将文件或文件夹拷贝到指定目录。
- 与系统文件剪贴板互通。
- 复制冲突自动生成唯一名称，不静默覆盖。
- 拒绝将文件夹复制到自身内部。
- Windows 提供 Shell 原生右键菜单和第三方扩展入口。

### 预览与编辑

- 图片、文本、TXT、Markdown 和代码预览。
- Windows 提供独立预览窗口和 GPUI 文本编辑器。
- macOS 使用 Quick Look、原生预览卡和 `NSTextView` 编辑器。
- 预览信息支持选择和复制。
- 文本编辑器支持未保存状态保护和外部修改冲突检测。

### 开发者工作流

- 显示 Git 和 SVN 文件状态。
- 文件夹显示子项目中优先级最高的状态。
- 右键菜单支持 Git 和 SVN 常用操作。
- Windows 可调用 TortoiseGit、TortoiseSVN 和 Shell 扩展。
- Windows 搜索和文件读取在后台执行，界面线程不等待磁盘响应。

### 原生平台集成

- Windows：Explorer 文件剪贴板、回收站、Shell 菜单和默认文件夹打开方式。
- macOS：Finder 文件剪贴板、Quick Look、分享菜单、LaunchServices 和文件查看器配置。
- 浅色与深色主题。
- 收藏配置跨启动保留。
- 更新包使用 SHA-256 校验；更新失败时恢复旧版本。

## 平台支持

| 能力 | Windows | macOS |
| --- | --- | --- |
| 原生界面 | Rust + GPUI | SwiftUI + AppKit |
| 分栏导航 | 已支持 | 已支持 |
| 页签与会话恢复 | 已支持 | 已支持 |
| 详细信息列 | 已支持 | 已支持 |
| 分栏排序方向 | 已支持 | 已支持 |
| 分栏排序字段、筛选和分组 | 已支持 | 开发中 |
| 全局搜索与分栏搜索 | 已支持 | 开发中 |
| 文件预览 | GPUI 预览 | Quick Look |
| 文本编辑器 | GPUI 编辑器 | 原生 AppKit 编辑器 |
| 文件剪贴板 | Explorer | Finder |
| 设备浏览 | USB、WPD、MTP | 磁盘卷 |
| 默认文件夹打开方式 | 已支持 | 受 macOS 版本限制 |

macOS 默认文件查看器的行为随系统版本变化。
详细边界见 [默认文件管理器架构](docs/architecture/default-file-manager.md)。

## 当前限制

Kite 仍处于 `0.6.x` 活跃开发阶段。

- Windows 提供 x64 构建，macOS 提供 Apple Silicon 构建。
- 文件列表当前只支持单选。
- 分栏搜索、排序字段、筛选和分组暂未同步到 macOS。
- Windows 搜索只匹配项目名称，最多显示 1000 个结果。
- WPD/MTP 设备当前支持浏览，不支持写入操作。
- 复制操作不支持符号链接。
- 当前版本不提供自定义列布局。

## 后续方向

- 对齐 macOS 分栏搜索和显示规则。
- 增加批量选择与批量文件操作。
- 增加便携设备写入能力。
- 增加自定义列和布局配置。
- 继续完善 Windows 与 macOS 的功能矩阵。

## 安装与更新

打开 `Kite > 检查更新…`，或在工具栏设置菜单中执行同一操作。
Kite 只会在用户主动检查更新时访问 GitHub Release。
下载完成后，Kite 校验 SHA-256，再替换当前版本。
更新失败时，安装器恢复旧版本。

Windows 更新日志位于 `%LOCALAPPDATA%\Kite\updates\update.log`。
macOS 更新日志位于 `~/Library/Application Support/Kite/updates/update.log`。

## 从源码构建

### Windows

需要稳定版 Rust、Visual Studio 2022 C++ 生成工具和 Windows SDK。

```powershell
cargo run --features windows-ui
```

打开指定目录：

```powershell
cargo run --features windows-ui -- "C:\Work\Project"
```

生成发布包：

```powershell
.\build.ps1
```

输出：

- `dist\Kite.exe`
- `dist\Kite-v<version>-windows-x64.zip`
- `dist\SHA256SUMS.txt`

### macOS

需要稳定版 Rust、Xcode 26 或更新版本和 Xcode Command Line Tools。

```bash
bash apps/macos/build.sh
```

输出：`apps/macos/build/Kite.app`

### 验证

```powershell
cargo fmt --all -- --check
cargo test --locked --lib
cargo test --locked --features windows-ui
cargo clippy --locked --all-targets --features windows-ui -- -D warnings
```

## 架构

```text
                  ┌──────────────────────┐
                  │      Rust Core       │
                  │ files · search · fs  │
                  │ session · VCS · sync │
                  └──────────┬───────────┘
                             │ UniFFI
                ┌────────────┴────────────┐
                │                         │
      ┌─────────▼─────────┐     ┌─────────▼─────────┐
      │    Windows UI     │     │      macOS UI     │
      │    Rust + GPUI    │     │  SwiftUI + AppKit │
      └───────────────────┘     └───────────────────┘
```

- Rust Core 负责文件读取、搜索、排序、会话、收藏、更新和 VCS。
- Windows 前端负责 GPUI 界面、Windows Shell、原生菜单、系统剪贴板和 Explorer 集成。
- macOS 前端负责 SwiftUI 界面、AppKit 窗口、系统剪贴板、Quick Look 和 Finder 集成。
- 两端不共享 UI 渲染器，只共享经过类型化的 Core 能力。

更多设计说明见 [平台边界](docs/architecture/platforms.md) 和 [项目宪法](docs/design/manifesto.md)。

## 文档

- [项目宪法与设计原则](docs/design/manifesto.md)
- [平台边界](docs/architecture/platforms.md)
- [右键菜单架构](docs/architecture/context-menu.md)
- [默认文件管理器](docs/architecture/default-file-manager.md)
- [更新器架构](docs/architecture/updater.md)
- [收藏架构](docs/architecture/favorites.md)
- [图标系统](docs/design/icon-system.md)
- [搜索实现调研](docs/RUST_SEARCH_ENGINE_RESEARCH.md)
- [文本编辑器调研](docs/TEXT_EDITOR_RESEARCH.md)
- [截图隐私规则](docs/SCREENSHOT_POLICY.md)

## 参与贡献

欢迎提交 Issue 和 Pull Request。

提交 PR 前：

1. 说明改动影响 Windows、macOS，还是两端。
2. 补上平台对齐说明。
3. 运行格式检查、测试和 Clippy。
4. UI 改动附带清晰的验证结果。

[提交 Issue](https://github.com/wyz541/kite/issues) · [查看 Pull Requests](https://github.com/wyz541/kite/pulls)

## 许可证

Kite 使用 MIT License。
见 [LICENSE](LICENSE)。