# Kite 默认文件管理器集成

## 结论

Kite 支持接管文件夹打开操作。
Windows 和 macOS 使用不同的原生机制。

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
LaunchServices 只设置内容类型的默认应用。

因此，两端只接管文件夹打开操作。
两端不替换桌面、任务栏、Dock 或系统外壳。

## Windows

### 注册位置

Kite 写入当前用户注册表。

```text
HKEY_CURRENT_USER\Software\Classes\Folder\shell\open\command
```

命令格式如下：

```text
"C:\Path\Kite.exe" "%1"
```

`%1` 是 Shell 传入的文件夹路径。

### 备份位置

Kite 将恢复信息保存在以下位置：

```text
HKEY_CURRENT_USER\Software\Kite\ExplorerIntegration
```

备份包含以下数据：

- 设置前的命令键是否存在。
- 设置前的默认命令值是否存在。
- 设置前的默认命令值。
- Kite 注册的命令。
- 备份格式版本。

### 启用流程

1. 读取当前用户的文件夹打开命令。
2. 读取已有 Kite 备份。
3. 如果当前命令仍是备份中的 Kite 命令，保留原备份。
4. 如果当前命令已由其他应用更改，将当前命令保存为新备份。
5. 写入 Kite 命令。
6. 保存备份。
7. 通知 Shell 关联已变化。

备份写入失败时，Kite 恢复原命令。

### 恢复流程

1. 读取 Kite 备份。
2. 读取当前文件夹打开命令。
3. 如果当前命令不是 Kite 命令，停止恢复。
4. 如果备份键原本不存在，删除 Kite 创建的命令键。
5. 如果备份值原本不存在，删除 Kite 写入的默认值。
6. 如果备份命令存在，写回备份命令。
7. 清除 Kite 备份。
8. 通知 Shell 关联已变化。

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

### 设置流程

Kite 调用以下 AppKit API：

```swift
NSWorkspace.shared.setDefaultApplication(
    at: Bundle.main.bundleURL,
    toOpen: .folder
)
```

系统可以显示用户同意提示。

### 恢复流程

Kite 将文件夹默认应用设置回 Finder。

```swift
NSWorkspace.shared.setDefaultApplication(
    at: finderApplicationURL,
    toOpen: .folder
)
```

### 打开请求

macOS 通过 AppDelegate 接收打开 URL。
Kite 过滤文件夹 URL。
每个文件夹打开一个新页签。

冷启动请求进入待处理队列。
SwiftUI 内容视图出现后处理待处理队列。

### macOS 边界

- Finder 仍是系统外壳。
- Finder 窗口仍由 Finder 管理。
- Dock、桌面和系统文件面板仍由 macOS 管理。
- LaunchServices 只改变文件夹默认打开应用。

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
- 启用时保存旧命令。
- 重复启用保留原备份。
- 恢复写回旧命令。
- 旧命令不存在时恢复为不存在。
- 外部修改后拒绝覆盖。

macOS CI 覆盖以下检查：

- SwiftUI 和 AppKit 编译。
- LaunchServices API 使用当前 SDK。
- `public.folder` 声明有效。
- 原生 UI 边界检查。
