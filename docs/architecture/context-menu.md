# Kite 右键菜单架构

## 结论

Kite 的右键菜单分为两层：

- Kite 原生菜单负责高频文件操作和 VCS 操作。
- Windows Shell 菜单负责系统功能、已安装扩展和第三方应用的命令。

这个分层避免 Kite 重写全部 Windows 功能。
这个分层也保留快速菜单和原生系统扩展。

## 跨平台边界

右键菜单共享命令语义，不共享菜单渲染器。

- Windows 使用 GPUI 和 Win32 Shell 扩展。
- macOS 使用 SwiftUI `contextMenu` 和 AppKit 菜单。
- macOS 不链接 GPUI。
- macOS 使用 AppKit 执行 Finder、Quick Look、分享和废纸篓操作。

本文件中的 `IContextMenu` 流程只属于 Windows 适配器。
跨平台总览见 `docs/architecture/platforms.md`。

## 问题

旧实现把核心操作和 Git/SVN 操作写在两个菜单构建位置。
旧实现没有统一的菜单模型。
旧实现没有调用 Windows Shell 的完整菜单。
因此，Kite 只能显示手写的少量功能。

## Windows 操作系统模型

### Shell 命名空间项目

Windows 用 `IShellItem` 表示文件、文件夹和虚拟项目。
Kite 用 `SHCreateItemFromParsingName` 从路径创建 `IShellItem`。
Windows 还提供 `IShellItemArray`。
`IShellItemArray` 可用于单项目和多项目菜单。

### 上下文菜单提供者

Windows 用 `IContextMenu` 生成传统 Shell 右键菜单。
`IContextMenu::QueryContextMenu` 添加菜单项。
`IContextMenu::InvokeCommand` 执行选中的命令。
应用可通过 `IShellItemArray::BindToHandler` 获取 `IContextMenu`。
TortoiseGit、TortoiseSVN、压缩工具和同步工具可注册到这条路径。

### 菜单消息

部分 Shell 扩展需要 `IContextMenu2` 或 `IContextMenu3`。
这些扩展需要接收 `WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM` 和 `WM_MENUCHAR`。
Kite 在弹出菜单期间临时子类化窗口。
Kite 将以上消息转发给 `IContextMenu3::HandleMenuMsg2`。
Kite 在旧扩展上回退到 `IContextMenu2::HandleMenuMsg`。

### 原生弹出菜单

Kite 用 `CreatePopupMenu` 创建菜单。
Kite 用 `TrackPopupMenuEx` 显示菜单。
Kite 用 `TPM_RETURNCMD` 获取选中命令。
Kite 按 `SM_MENUDROPALIGNMENT` 选择左对齐或右对齐。
Kite 调用 `SetForegroundWindow`。
因此，用户点击菜单外部时，菜单会正常关闭。

### COM 线程

Shell 菜单位于 UI 线程。
GPUI 的 Windows 平台初始化已调用 `OleInitialize`。
因此，Kite 不在后台线程创建 Shell 菜单。
菜单命令可以启动后台程序。

## Kite 架构

### ContextMenuTarget

`ContextMenuTarget` 描述菜单目标。

```text
path
title
is_dir
loading
can_paste
git_available
svn_available
```

菜单构建不读取全局选择状态。
动作携带目标路径。
因此，菜单目标变化时，动作不会误用旧选择。

### ContextMenuModel

`ContextMenuModel` 是与 GPUI 无关的菜单树。
菜单节点只有三种：

- 分隔线。
- 命令项。
- 子菜单。

模型负责顺序、能力判断和分隔线清理。
渲染器只负责把模型转换为 `PopupMenu`。

### MenuProvider

每个提供者只负责一个功能域。

```text
CoreMenuProvider
VcsMenuProvider
NativeShellMenuProvider
RefreshMenuProvider
```

当前顺序是固定顺序。
未来可改为按能力注册。
新增功能只需添加提供者或模型节点。
调用点不需要再复制菜单代码。

### Action Envelope

每个菜单动作使用 `ContextMenuAction`。

```text
target: PathBuf
working_directory: PathBuf
command: ContextMenuCommand
```

动作分发集中在 `KiteApp::on_context_menu_action`。
因此，列表视图、分栏视图和预览菜单使用同一套命令语义。

### 预览更多菜单

分栏预览标题栏提供“更多预览选项”按钮。
该按钮复用 `ContextMenuAction` 和 `ContextMenuCommand`。
当前菜单包含：

- 打开文件。
- 复制。
- 复制完整路径。
- 文件属性。
- 显示原生菜单。

`CopyPath` 写入系统文本剪贴板。
`ShowProperties` 调用 Windows Shell 的 `SHObjectProperties`。
该调用使用 `SHOP_FILEPATH` 对象类型。
因此，属性对话框使用系统实现。
后续预览操作只新增命令和菜单节点。
后续预览操作不需要复制动作分发代码。

### Native Shell Bridge

Windows 模块位于 `src/context_menu/windows.rs`。
桥接流程如下：

1. 从路径创建 `IShellItem`。
2. 创建 `IShellItemArray`。
3. 通过 `BHID_SFUIObject` 获取 `IContextMenu`。
4. 调用 `QueryContextMenu`，并请求正常动词和扩展动词。
5. 安装 `IContextMenu2/3` 消息转发。
6. 调用 `TrackPopupMenuEx`。
7. 用 `CMINVOKECOMMANDINFOEX` 调用 `InvokeCommand`。
8. 销毁菜单并移除窗口子类。

### 延迟调度

GPUI 动作在 `App::update` 调用栈中执行。
`TrackPopupMenuEx` 会启动嵌套 Windows 消息循环。
因此，动作处理中不能同步打开原生菜单。
同步打开会导致窗口消息重入 GPUI 更新。
进程随后以 `FAST_FAIL_FATAL_APP_EXIT` 中止。

Kite 用 `PostMessageW` 投递私有窗口消息。
GPUI 更新返回后，窗口子类处理该消息。
窗口子类随后打开原生菜单。
菜单关闭后，一次性结果通道唤醒 GPUI 任务。
GPUI 任务再刷新文件和版本控制状态。

### 一级菜单保持

`显示更多选项` 使用悬停触发。
悬停处理先聚焦 Kite 根动作上下文。
随后派发 `SystemMenu` 动作。
原生菜单作为二级浮层显示。
一级 PopupMenu 不收到取消事件。
因此，一级菜单在原生菜单打开期间保持可见。
鼠标点击仍保留普通动作路径。

## 功能边界

### Kite 原生项

Kite 原生项适合高频操作。

- 打开。
- 复制。
- 复制完整路径。
- 剪切。
- 粘贴。
- 新建文件夹。
- 新建文件。
- 重命名。
- 移到回收站。
- 文件属性。
- Git 操作。
- SVN 操作。
- 刷新。

### Windows 原生菜单

`显示更多选项 (Show more options)` 适合低频和扩展功能。

- 打开方式。
- 属性。
- 发送到。
- 压缩和解压。
- 同步和云盘扩展。
- TortoiseGit 菜单。
- TortoiseSVN 菜单。
- 已安装的第三方 Shell 扩展。

## Windows 11 说明

Windows 11 引入 `IExplorerCommand`。
现代菜单可通过 `IExplorerCommand` 提供标题、图标、状态和子命令。
`IContextMenu` 仍支持传统 Shell 扩展。
当前桥接目标是 `IContextMenu` 兼容菜单。
因此，传统扩展功能可用。
`CMF_EXTENDEDVERBS` 增加扩展动词。
Windows 11 新式紧凑菜单不保证由 `IContextMenu` 返回。
后续版本应增加 `IExplorerCommand` 提供者适配层。

## 当前限制

- 当前动作目标是单一路径。
- 当前列视图和列表视图共享单选择语义。
- 当前尚未提供文件夹背景菜单。
- 当前原生 Shell 菜单是同步弹出菜单。
- 当前尚未缓存 Shell 扩展能力。

## 后续阶段

### P1 多选

1. 将 `ContextMenuTarget` 扩展为路径集合。
2. 用 `SHCreateShellItemArrayFromIDLists` 创建多项目集合。
3. 增加批量文件和 VCS 动作。

### P2 背景菜单

1. 获取当前 `IShellFolder`。
2. 调用 `CreateViewObject` 获取背景 `IContextMenu`。
3. 增加空目录和列表空白区域菜单。

### P3 Windows 11 命令

1. 定义 `ExplorerCommandProvider`。
2. 枚举 `IExplorerCommand`。
3. 合并现代命令和传统命令。
4. 保留功能去重规则。

### P4 可观测性

1. 记录菜单提供者耗时。
2. 记录 Shell 扩展失败。
3. 增加扩展禁用列表。
4. 增加菜单诊断页。

## 安全与稳定性

- Shell 菜单在 UI 线程调用。
- Shell 菜单操作不写入文件内容。
- Shell 命令由 Windows 或扩展执行。
- Kite 只验证目标路径和调用结果。
- 调度子类在消息处理完成或窗口销毁时移除。
- 菜单句柄和子类数据使用 RAII 清理。
- 原生菜单不在 GPUI 更新调用栈中打开。
- 同一时间只允许一个待处理的原生菜单请求。
- 后台 VCS 操作继续使用 `CREATE_NO_WINDOW`。

## 验证

- 模型测试检查 Git 一级菜单。
- 模型测试检查 SVN 一级菜单。
- 模型测试检查 Git 完整动作。
- 模型测试检查 SVN 完整动作。
- 模型测试检查新建文件菜单项。
- 模型测试检查加载状态禁用规则。
- 原生桥接测试创建 Win32 窗口。
- 原生桥接测试打开 Shell 菜单并自动关闭。
- 调试器回归测试选择原生 Shell 命令，确认进程继续运行。
- 模型测试检查 `显示更多选项 (Show more options)` 入口。

## 主要来源

- [IContextMenu](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-icontextmenu)
- [IContextMenu2](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-icontextmenu2)
- [IContextMenu3](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-icontextmenu3)
- [IExplorerCommand](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-iexplorercommand)
- [IShellItemArray](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ishellitemarray)
- [IShellItemArray::BindToHandler](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishellitemarray-bindtohandler)
- [SHCreateItemFromParsingName](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-shcreateitemfromparsingname)
- [CMINVOKECOMMANDINFOEX](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/ns-shobjidl_core-cminvokecommandinfoex)
- [TrackPopupMenuEx](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-trackpopupmenuex)
- [SetWindowSubclass](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-setwindowsubclass)
