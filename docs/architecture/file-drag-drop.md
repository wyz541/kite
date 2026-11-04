# 文件拖放

## 结论

Windows 和 macOS 支持拖动文件与文件夹。
拖放目标包括文件夹行和分栏背景。
Kite 支持内部移动、内部复制、从系统拖入和向系统拖出。

## Windows

### 拖出

文件行和文件夹行提供内部拖放数据。
内部拖放包含当前选中项目。
指针离开 Kite 窗口后，Kite 结束 GPUI 内部拖动。
Kite 向窗口投递 `WM_APP` 消息。
Windows 消息泵在当前事件返回后处理该消息。
消息处理程序调用 `SHDoDragDrop`。
`SHDoDragDrop` 使用 Kite 的真实窗口句柄。
消息处理程序让 Shell 创建默认拖动源。
原生拖放使用系统文件数据对象。
系统文件数据对象提供 `CF_HDROP`。
`CF_HDROP` 包含真实文件和文件夹的完整路径。
原生文件拖放使用复制效果。

### 拖入

文件夹行接收文件或文件夹。
分栏背景接收文件或文件夹。
内部拖放默认移动项目。
按住 Ctrl 拖放时复制项目。
从 Explorer 拖入时复制项目。
移动设备命名空间暂不支持拖入。

## macOS

### 拖出

文件行和文件夹行提供 `NSItemProvider`。
Provider 同时包含系统文件 URL 和 Kite 内部拖放类型。
系统文件 URL 排在内部 JSON 数据之前。
系统文件 URL 是首个拖放类型。
Kite 内部拖放包含当前选中项目。
Finder 和其他应用读取系统文件 URL。
网页上传目标把拖放内容识别为文件，不识别为文本。

### 拖入

文件夹行接收文件或文件夹。
分栏背景接收文件或文件夹。
内部拖放复用剪切和粘贴命令。
默认内部拖放执行剪切和粘贴。
按住 Option 拖放执行拷贝和粘贴。
从 Finder 拖入时复制项目。
剪切、拷贝和粘贴完成后刷新全部可见分栏。
工具栏和菜单刷新重新读取每个可见分栏。

## 约束

拖放使用共享文件传输接口。
复制会生成不冲突的目标名称。
文件夹不能移动或复制到自身内部。
移动完成后刷新全部可见分栏。
拖放失败时显示错误信息。

## 验证

- Windows GPUI build。
- Windows Core tests。
- Windows Clippy。
- macOS SwiftUI and AppKit build。
- macOS Core tests。
- macOS Clippy。
- macOS Finder drag provider smoke check：`bash apps/macos/test-file-drag.sh`。
- Windows Explorer drag target smoke check。
- Windows Chrome and Edge upload E2E：`.github/scripts/windows-drag-upload-e2e.ps1`。
