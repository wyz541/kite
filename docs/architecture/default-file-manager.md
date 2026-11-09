# Kite 默认文件管理器集成

## 结论

Kite 支持默认文件管理器集成。
Windows 直接接管文件夹打开操作。
macOS 使用系统允许的文件查看器机制。

系统外壳保持不变。
Windows 继续运行 Explorer。
macOS 继续运行 Finder。

## 设计目标

- 用户主动启用默认文件管理器。
- 不要求管理员权限。
- 保存设置前的打开命令。
- 支持恢复设置前的打开命令。
- 不覆盖其他应用后来设置的默认值。
- 文件夹路径通过参数进入 Kite。

## 设计约束

Windows 没有公开的“默认文件管理器”用户设置。
文件夹打开由 Shell 文件关联决定。

macOS 没有公开的 Finder 替换机制。
macOS 13 到 15 允许设置 `public.folder` 默认应用。
macOS 26 及更高版本拒绝第三方应用接管文件夹双击。

两端不替换桌面、任务栏、Dock 或系统外壳。

## Windows

### 注册位置

Kite 写入当前用户注册表。

```text
HKEY_CURRENT_USER\Software\Classes\Folder\shell\open\command
HKEY_CURRENT_USER\Software\Classes\Folder\shell\explore\command
```

两个命令都使用以下格式：

```text
"C:\Path\Kite.exe" "%1"
```

`%1` 是 Shell 传入的文件夹路径。

Kite 向两个命令键写入空 `DelegateExecute` 值。
Windows 系统级 `open` 和 `explore` 命令使用 Explorer 的委托执行器。
当前用户命令必须阻止该委托继承。
否则 Shell 会忽略 Kite 命令。

### 备份位置

Kite 将恢复信息保存在以下位置：

```text
HKEY_CURRENT_USER\Software\Kite\ExplorerIntegration
```

备份包含以下数据：

- `open` 和 `explore` 命令键是否存在。
- 两个键的默认命令值是否存在。
- 两个键的默认命令值。
- 两个键的 `DelegateExecute` 值是否存在。
- 两个键的 `DelegateExecute` 值和类型。
- Kite 注册的命令。
- 备份格式版本。

旧版备份格式仍可恢复。
旧版备份不管理 `explore` 和 `DelegateExecute`。

### 启用流程

1. 读取当前用户的 `open` 命令。
2. 读取当前用户的 `explore` 命令。
3. 读取已有 Kite 备份。
4. 如果当前命令仍是备份中的 Kite 命令，保留原备份。
5. 如果当前命令已由其他应用更改，将当前命令保存为新备份。
6. 写入两个 Kite 命令。
7. 清空两个命令键的 `DelegateExecute`。
8. 保存备份。
9. 通知 Shell 关联已变化。

备份写入失败时，Kite 恢复原命令和原 `DelegateExecute` 值。

### Windows Search 和 PowerToys Run

ZIP 便携版没有安装步骤。
Windows Search 和 PowerToys Run 默认不扫描任意文件夹中的 `.exe`。

Kite 提供当前用户注册入口。
入口包含以下内容：

- `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\App Paths\Kite.exe`
- `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Kite.lnk`

设置默认资源管理器时，Kite 自动创建该入口。
用户也可以执行 `Kite > 创建开始菜单快捷方式…`。
快捷方式指向当前运行的 `Kite.exe`。

PowerToys Run 缓存应用列表。
注册后需要重启 PowerToys Run 或重新加载应用列表。

### 恢复流程

1. 读取 Kite 备份。
2. 读取当前 `open` 命令。
3. 读取当前 `explore` 命令。
4. 如果任一当前命令不是 Kite 命令，停止恢复。
5. 恢复两个命令键的存在状态。
6. 恢复两个默认命令值。
7. 恢复两个 `DelegateExecute` 值。
8. 清除 Kite 备份。
9. 通知 Shell 关联已变化。

恢复流程不覆盖其他应用后来设置的默认值。

### Windows 边界

- Explorer 仍是系统外壳。
- 桌面仍由 Explorer 管理。
- 任务栏仍由 Explorer 管理。
- `Win+E` 仍打开 Explorer。
- Windows 系统文件对话框仍使用系统实现。
- 清空回收站、控制面板和虚拟文件夹仍使用 Windows Shell。

## macOS

### 注册声明

`Info.plist` 声明 `public.folder` 文档类型。
`LSHandlerRank` 使用 `Alternate`。

### 设置流程：macOS 13 到 15

Kite 调用以下 AppKit API：

```swift
NSWorkspace.shared.setDefaultApplication(
    at: Bundle.main.bundleURL,
    toOpen: .folder
)
```

系统可以显示用户同意提示。

### 设置流程：macOS 26 及更高版本

Kite 写入当前用户的全局文件查看器偏好：

```text
NSFileViewer = com.wyz541.kite
```

macOS 将这些请求发送到 Kite：

- 其他应用的“在 Finder 中显示”请求。
- Finder 的 `show` Apple Event。
- 系统文件查看请求。

Kite 注册 `misc/mvis` 和 `FNDR/show` Apple Event 处理器。
文件请求打开父目录并选中目标文件。
文件夹请求打开新页签。
Finder 继续处理文件夹双击、Dock 和桌面。

### 恢复流程

Kite 删除 `NSFileViewer` 偏好。
macOS 13 到 15 还会将文件夹默认应用设置回 Finder。

```swift
NSWorkspace.shared.setDefaultApplication(
    at: finderApplicationURL,
    toOpen: .folder
)
```

### 打开请求

macOS 通过 AppDelegate 接收打开 URL。
文件夹打开新页签。
文件打开父目录并选中文件。
Apple Event 和普通打开请求共用同一个待处理队列。

冷启动请求进入待处理队列。
SwiftUI 内容视图出现后处理待处理队列。

### macOS 边界

- Finder 仍是系统外壳。
- Finder 窗口仍由 Finder 管理。
- Dock、桌面和系统文件面板仍由 macOS 管理。
- macOS 26 及更高版本保留 Finder 的文件夹双击行为。
- `NSFileViewer` 只改变其他应用的“在 Finder 中显示”目标。

## 安全要求

- 只写入当前用户设置。
- 不请求管理员权限。
- 不覆盖无法确认的注册表键。
- 不删除非 Kite 命令键。
- 不在默认值变化后强制恢复。
- 设置和恢复都要求用户确认。

## 测试

Windows 测试覆盖以下行为：

- 命令路径使用引号。
- 启用时保存 `open` 和 `explore` 旧命令。
- 启用时清空两个 `DelegateExecute` 值。
- 重复启用保留原备份。
- 恢复写回两个旧命令。
- 恢复写回两个旧 `DelegateExecute` 值。
- 旧命令不存在时恢复为不存在。
- 外部修改后拒绝覆盖。
- 旧版备份只恢复 `open` 命令。

macOS CI 覆盖以下检查：

- SwiftUI 和 AppKit 编译。
- 文件查看器偏好和 Apple Event 路由编译。
- `public.folder` 声明有效。
- 原生 UI 边界检查。
