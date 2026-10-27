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

Rust Core 输出 `rlib`、`staticlib` 和 `cdylib`。
Swift 通过 UniFFI 生成绑定。
Windows UI 直接使用 Rust crate。

## Windows 前端

Windows 前端继续使用 Rust 和 GPUI。

Windows 前端负责以下能力：

- GPUI 窗口和表格。
- 多页签。
- 页签会话快照。
- 分栏滚动。
- Kite 原生右键菜单。
- `IContextMenu` Shell 扩展菜单。
- WPD/MTP 设备展示。

Windows 专属代码只在 `cfg(target_os = "windows")` 下编译。

## macOS 前端

macOS 前端使用 SwiftUI 和 AppKit。
macOS 26 或更新系统在功能层使用 Liquid Glass。
macOS 13 到 15 使用标准材质降级。

SwiftUI 负责以下能力：

- Liquid Glass 工具栏和路径控件。
- 页签栏。
- 窗口和工具栏。
- 分栏视图。
- 路径输入。
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
- 文件条目。
- Git 和 SVN 状态。
- 卷和设备快照。
- 会话读取和保存。

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
菜单栏的“导航”菜单只负责聚焦路径输入栏。
工具栏的上一级按钮负责父目录导航。
工具栏禁止增加重复的“前往”下拉菜单。
菜单栏禁止增加重复的路径输入或父目录命令。

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
| 移动设备 | WPD 枚举 + Windows Shell 打开 | macOS 卷枚举 |
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
- macOS 26 使用 Liquid Glass 工具栏。
- macOS 13 到 15 使用标准材质工具栏。
- macOS Git/SVN 完整菜单仍待接入共享命令模型。
- macOS Finder Sync 扩展未实现。
- macOS Universal Binary 未实现。
- macOS 公证未实现。
- Windows 和 macOS 文件操作仍需增加跨平台回归测试。
