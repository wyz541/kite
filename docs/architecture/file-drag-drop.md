# 文件拖放

## 结论

Windows 和 macOS 支持拖动文件与文件夹。
拖放目标包括文件夹行和分栏背景。
Kite 支持内部移动、内部复制、从系统拖入和向系统拖出。

## Windows

### 拖出

文件行和文件夹行提供内部拖放数据。
内部拖放包含当前选中项目。
指针离开 Kite 窗口后，Kite 启动原生 OLE 文件拖放。
原生拖放使用 Shell 数据对象。
Shell 数据对象包含真实文件和文件夹项目。
Shell 数据对象使用桌面为根的完整 PIDL。
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
内部拖放默认移动项目。
按住 Option 拖放时复制项目。
从 Finder 拖入时复制项目。
拖放完成后，Kite 打开目标目录并显示传入项目。

## 约束

拖放使用共享文件传输接口。
复制会生成不冲突的目标名称。
文件夹不能移动或复制到自身内部。
移动完成后刷新当前分栏链。
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
