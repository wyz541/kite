# Kite 跨平台架构

## 结论

Kite 不是单一 UI 跨平台应用。
Kite 是共享 Rust Core 加两个原生 UI 前端。

| 平台 | UI | 系统集成 |
|---|---|---|
| Windows | Rust + GPUI | Win32、Shell、WPD |
| macOS | SwiftUI + AppKit | AppKit、Finder、Quick Look、Services |

两端共享领域模型和文件操作。
两端不共享 UI 渲染器。

macOS 原生 UI 是视觉与交互基准。
Windows GPUI UI 按 macOS Kite UI 实现。
该边界由 [Kite 项目宪法](../design/manifesto.md) 约束。

## 模块边界

```text
kite_core
  fs
  vcs
  devices
  platform paths
  command model
  session store
  UniFFI interface

Windows frontend
  GPUI views
  GPUI tabs
  Windows Shell bridge

macOS frontend
  SwiftUI views
  SwiftUI tabs
  AppKit menu and workspace
  UniFFI Swift bindings
```

## Rust Core

Rust Core 是第一公民共享层。
Core 不依赖 GPUI、SwiftUI 或 AppKit。

Core 负责以下能力：

- 文件夹读取。
- 排序和搜索。
- 文件操作。
- 文件状态。
- Git 和 SVN 状态。
- Git 和 SVN 命令。
- 设备和卷发现。
- 平台路径规则。
- 稳定命令标识。
- 跨语言数据记录。
- 会话保存和恢复。
- GitHub Release 检查和版本比较。
- 平台更新资产选择和 SHA-256 校验。

Rust Core 输出 `rlib`、`staticlib` 和 `cdylib`。
Swift 通过 UniFFI 生成绑定。
Windows UI 直接使用 Rust crate。
更新安装由平台轻量助手执行。
详细设计见 `docs/architecture/updater.md`。

## Windows 前端

Windows 前端继续使用 Rust 和 GPUI。

Windows 前端负责以下能力：

- GPUI 窗口和表格。
- 多页签。
- 页签会话快照。
- 分栏滚动。
- 分栏批量选择。
- 全局工具栏搜索。
- 自定义详细信息列和列宽预设。
- Kite 原生右键菜单。
- `IContextMenu` Shell 扩展菜单。
- WPD/MTP 设备展示。
- WPD/MTP Shell 命名空间浏览和写入。

Windows 专属代码只在 `cfg(target_os = "windows")` 下编译。

## WPD/MTP 浏览与写入

Windows 设备条目来自 WPD 枚举。
设备路径是 Shell 命名空间路径，不是文件系统路径。
`src/shell_fs.rs` 使用 `IShellItem` 和 `IEnumShellItems` 枚举目录。
后台线程先调用 `CoInitializeEx`，再执行 Shell 枚举。
设备内导航复用 Kite 的历史、路径栏和分栏状态。
设备文件通过 Windows Shell 打开。
设备写入使用 `IFileOperation`。
Kite 支持复制、移动、新建、重命名和删除设备项目。
设备项目复制使用 Kite 内部剪贴板。
设备删除是永久操作。设备删除不进入回收站。
Kite 使用 Shell 属性判断重命名和删除权限。
设备根目录禁止重命名和删除。
设备操作由 Windows Shell 执行。设备固件和驱动决定操作兼容性。

## macOS 前端

macOS 前端使用 SwiftUI 和 AppKit。
macOS 26 或更新系统在导航外壳使用 Liquid Glass。
玻璃表面覆盖页签栏、工具栏、侧边栏、选中行、预览栏和状态栏。
macOS 13 到 15 使用标准材质降级。

SwiftUI 负责以下能力：

- Liquid Glass 导航外壳、侧边栏、预览栏和状态栏。
- 页签栏。
- 窗口和工具栏。
- 标准位置、收藏和 Rust 卷枚举侧边栏。
- 分栏视图。
- 分栏批量选择。
- 全局工具栏搜索。
- 自定义详细信息列、列宽预设和布局恢复。
- 面包屑和路径输入。
- 状态栏。
- App commands。
- 窗口关闭和退出时的会话保存。

AppKit 负责以下能力：

- `NSWorkspace` 打开和 Finder 定位。
- `NSMenu` 和服务菜单。
- `NSSharingServicePicker`。
- Quick Look。
- `NSPasteboard`。
- `NSFileManager` 废纸篓操作。

macOS 前端不链接 GPUI。
macOS 前端不渲染 egui。

## UniFFI 接口

Swift 不直接读取 Rust 私有结构。
Swift 使用 UniFFI 记录和对象。

当前接口提供以下能力：

- Core 版本。
- 平台名称。
- 默认目录。
- 目录列表。
- 目录递归搜索。
- 文件条目。
- 文件复制和移动。
- Git 和 SVN 状态。
- 卷和设备快照。
- 收藏读取和修改。
- 会话读取和保存。
- 文本文件读取和保存。
- 更新检查、下载进度和更新包校验。

后续接口按功能域增加。
接口不暴露 GPUI 类型。
接口不暴露 AppKit 类型。

## 右键菜单

右键菜单共享命令语义，不共享渲染器。

### 共享层

- 稳定命令标识。
- 命令标签。
- 命令可用性。
- 命令分组。
- 目标路径。
- VCS 能力。

### Windows 渲染

- GPUI 菜单显示 Kite 高频命令。
- `Windows 原生菜单…` 打开 Shell 扩展菜单。
- Shell 命令由 `IContextMenu` 执行。

### macOS 渲染

- SwiftUI `contextMenu` 显示 Kite 高频命令。
- AppKit 执行 Finder、Quick Look、分享和废纸篓操作。
- AppKit 服务菜单提供系统服务。
- macOS UI 按 HIG 隐藏不可用项。
- macOS 菜单控制分组和子菜单深度。

## 导航入口

Kite 只保留一个路径跳转入口。
路径输入栏负责绝对路径、相对路径、`~` 和 `/`。
菜单栏的“导航”菜单提供路径聚焦和全局搜索入口。
工具栏的上一级按钮负责父目录导航。
工具栏禁止增加重复的“前往”下拉菜单。
菜单栏禁止增加重复的路径输入或父目录命令。

## 分栏滚轮路由

Windows 和 macOS 使用相同的滚轮规则：

- 普通纵向滚轮只滚动指针所在分栏。
- 分栏到达顶部或底部后，剩余纵向事件停止。
- 纵向事件不得转换为分栏横向滚动。
- Shift 加纵向滚轮滚动分栏区域。
- 横向触控板手势滚动分栏区域。

Windows 使用 `ScrollableMask` 限制横向滚轮轴。
macOS 在纵向列表边界拦截剩余事件。

## 默认文件夹打开方式

Kite 支持将自身设为默认文件查看器。
Windows 使用当前用户 `Folder\shell\open\command`。
macOS 13 到 15 使用 LaunchServices `public.folder`。
macOS 26 及更高版本使用全局 `NSFileViewer` 偏好。

Windows 保存并恢复旧命令。
macOS 将文件查看器恢复为 Finder。
两端都不替换桌面、任务栏、Dock 或系统外壳。

详细设计见 `docs/architecture/default-file-manager.md`。

## 平台策略

不同平台使用不同 UI 策略。

| 项目 | Windows | macOS |
|---|---|---|
| 主 UI | GPUI | SwiftUI |
| 页签 | GPUI 页签 | SwiftUI 页签 |
| 功能层材质 | GPUI 主题 | Liquid Glass / 标准材质 |
| 菜单渲染 | GPUI + Win32 | SwiftUI + AppKit |
| 系统扩展 | Shell 扩展 | Services 和 AppKit |
| 文件打开 | Shell 默认程序 | NSWorkspace |
| 快速查看 | 无统一 API | Quick Look |
| 回收站 | Windows Recycle Bin | macOS Trash |
| 移动设备 | WPD 枚举 + Shell 命名空间读写 | macOS 卷枚举 + 文件系统写入 |
| 快捷键 | Ctrl 映射 | Command 映射 |
| 不可用菜单项 | 按现有要求显示禁用项 | 按 HIG 隐藏 |

## 构建与验收

Windows CI 执行以下检查：

- Rust format。
- Core tests。
- Windows UI tests。
- Clippy。
- Windows release build。

macOS CI 执行以下检查：

- Rust format。
- Core tests。
- Clippy。
- Rust dylib build。
- UniFFI Swift binding generation。
- macOS 26 Liquid Glass SDK check。
- SwiftUI + AppKit application build。
- macOS app packaging。

任一平台失败时，发布不通过。

## 当前边界

- macOS 和 Windows 均支持页签和会话恢复。
- 两端分栏视图均支持批量选择、全局搜索和列布局配置。
- Windows WPD/MTP 删除操作不进入回收站。
- macOS 26 使用 Liquid Glass 导航外壳。
- macOS 13 到 15 使用标准材质导航外壳。
- macOS Git/SVN 完整菜单仍待接入共享命令模型。
- macOS Finder Sync 扩展未实现。
- macOS Universal Binary 未实现。
- macOS 公证未实现。
- Windows 和 macOS 文件操作仍需增加跨平台回归测试。
