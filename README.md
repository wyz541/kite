# Kite

Kite 是一个跨平台文件管理器。
Windows 使用 Rust 和 GPUI。
macOS 使用 SwiftUI 和 AppKit。
macOS 26 使用 Liquid Glass。
macOS 13 到 15 使用标准材质降级。
两端共享 Rust Core。

macOS 不使用 GPUI，也不使用 egui。
macOS 的系统菜单、Quick Look、Finder 和分享操作由 AppKit 提供。
公开截图遵守 [截图隐私规则](docs/SCREENSHOT_POLICY.md)。

## 设计宣言

Kite 拒绝把文件管理做成沉重的“柜子”。
Kite 把界面当作一条可牵引的线。
线连接用户意图和数据流。
设计围绕中介、约束、视角和可逆性展开。
完整设计原则见 [docs/design/manifesto.md](docs/design/manifesto.md)。

## 当前能力

- 浏览本机文件夹。
- 使用多个页签替换多个窗口。
- 关闭窗口时保存页签和目录。
- 应用重新启动时恢复页签和目录。
- 工具栏不重复显示应用图标和应用名。
- 识别 USB 存储设备、可移动磁盘和 WPD/MTP 便携设备。
- 点击 WPD/MTP 设备可在 Windows 中打开设备根目录。
- 使用侧边栏访问主文件夹、桌面、文档、下载、图片和本机驱动器。
- 使用并排分栏浏览多级文件夹。
- 分栏导航只加载新增或缺失的层级。
- 使用可点击路径栏返回任意上级文件夹。
- 使用路径输入栏跳转，使用上一级按钮返回父目录。
- 在分栏视图任意栏选中文件时显示预览和属性。
- 预览属性文本支持选择，并可用 `Ctrl+C` 或 `Command+C` 复制。
- 预览标题栏提供“复制预览信息”按钮。
- 使用地址栏打开绝对路径或相对路径。
- 输入文件路径时，Kite 打开文件所在文件夹，并调用系统默认程序打开文件。
- 使用返回、前进和上一级按钮导航。
- 按名称、修改日期、类型和大小排序。
- 递归搜索当前文件夹。
- 双击文件夹进入下级文件夹。
- Windows 双击文件调用 Shell 默认程序。
- macOS 双击文件调用 `NSWorkspace`。
- 在 Windows 中将 Kite 设为默认文件夹打开方式。
- 在 macOS 中将 Kite 设为默认文件夹打开应用。
- 恢复设置前的文件夹打开方式。
- 新建文件夹、重命名和移到回收站。
- 复制、剪切和粘贴文件或文件夹。
- 右键文件行或分栏目录区域打开操作菜单。
- 在右键菜单中执行 Git 和 SVN 操作。
- 显示 Git 和 SVN 状态徽标。
- 在浅色主题和深色主题之间切换。
- 检查 GitHub Release，并更新本机 Kite。
- 过滤 Windows 隐藏属性和系统属性的项目。
- macOS 使用平台文件名规则和卷枚举。
- macOS 26 工具栏使用 Liquid Glass。
- macOS 13 到 15 工具栏使用标准材质。

## 软件更新

打开 `Kite > 检查更新…`。
工具栏设置菜单提供同一入口。
Kite 在后台读取 GitHub 最新稳定 Release。
Kite 使用语义化版本比较当前版本和最新版本。
如果 GitHub API 限流，Kite 使用 Release 页面跳转和 `SHA256SUMS.txt`。
更新包必须匹配当前平台。
Windows 更新包为 `Kite.exe`。
macOS 更新包为 `Kite-vX.Y.Z-macOS-arm64.zip`。
Kite 下载后校验 SHA-256。
校验失败时，Kite 不安装更新包。
Windows 安装器等待当前进程退出。
Windows 安装器备份当前 EXE，替换 EXE，然后重启 Kite。
macOS 安装器等待当前进程退出。
macOS 安装器使用 `ditto` 解压更新包，替换应用包，然后重启 Kite。
更新失败时，安装器恢复旧版本。
Windows 日志位于 `%LOCALAPPDATA%\Kite\updates\update.log`。
macOS 日志位于 `~/Library/Application Support/Kite/updates/update.log`。
检查和下载只在用户选择“检查更新…”后发生。

详细设计见 `docs/architecture/updater.md`。

## 默认资源管理器

Windows 用户打开 `Kite > 设为默认资源管理器…`。
工具栏的文件夹设置按钮提供同一入口。
Kite 将写入当前用户的 `Folder\shell\open\command`。
Kite 保存设置前的命令。
`Kite > 恢复之前的文件夹打开方式…` 恢复保存的命令。
Windows 桌面、任务栏、`Win+E` 和系统文件对话框仍由 Windows 管理。

ZIP 便携版默认不进入 Windows Search 或 PowerToys Run。
这两个工具搜索已安装应用、开始菜单快捷方式和 App Paths。
打开 `Kite > 创建开始菜单快捷方式…` 可注册当前用户入口。
“设为默认资源管理器”也会自动创建该入口。
Kite 写入当前用户 `App Paths\Kite.exe`，并创建 `Kite.lnk`。
重启 PowerToys Run 或重新加载应用列表后即可搜索 Kite。

macOS 用户打开 `文件夹 > 设为默认文件夹打开方式…`。
Kite 使用 LaunchServices 设置 `public.folder` 的默认应用。
`文件夹 > 恢复 Finder 为默认…` 恢复 Finder。
macOS 仍将 Finder 作为系统外壳。

详细设计见 `docs/architecture/default-file-manager.md`。

## 右键菜单

右键文件行后，Kite 先选中该行，再显示操作菜单。
右键分栏的列标题或空白区域后，Kite 对该列目录显示操作菜单。

菜单支持以下操作：

- 打开
- 复制
- 剪切
- 粘贴
- 新建文件夹
- 重命名
- 移到回收站
- 刷新
- 显示更多选项 (Show more options)

在 Git 工作区中，菜单增加 Git 添加、Git 提交和 Git 还原。
在 SVN 工作副本中，菜单增加 SVN 更新、SVN 添加、SVN 提交和 SVN 还原。

悬停 `显示更多选项 (Show more options)` 打开 Windows Shell 菜单。
原生菜单显示时，一级 Kite 菜单保持可见。
此菜单包含系统操作、第三方 Shell 扩展和扩展动词。

菜单显示对应快捷键。

## 版本控制状态

列表视图在“版本控制”列显示 Git 和 SVN 状态。Git 使用分支图标。SVN 使用堆叠图标。

| 代码 | 含义 |
| --- | --- |
| `M` | 已修改 |
| `A` | 已新增 |
| `D` | 已删除 |
| `R` | 已重命名 |
| `?` | 未跟踪 |
| `!` | 已忽略或缺失 |
| `C` | 存在冲突 |
| `P` | 属性已修改 |
| `X` | 外部项目 |

文件夹显示子项目中优先级最高的状态。冲突、缺失和删除状态的优先级最高。

状态查询在后台任务中执行。命令不可用或目录不属于工作副本时，状态列保持空白。

Kite 需要 `git` 和 `svn` 在 `PATH` 中。缺少对应命令时，Kite 不显示对应状态。

### Git 与 SVN 操作

Kite 只在 Git 工作区显示 Git 操作。Kite 只在 SVN 工作副本显示 SVN 操作。

Git 和 SVN 始终显示在右键菜单的一级菜单中。
子菜单在非对应工作区中保持可见，操作项显示为禁用。

Git 子菜单支持添加、取消暂存、提交、拉取、推送、获取、显示差异、显示日志、还原和清理工作区。
Git 提交使用所选路径的已暂存更改。
“显示日志”打开 TortoiseGit 日志窗口。
Kite 在 TortoiseGit 不可用时尝试 `gitk`。
GUI 都不可用时，Kite 打开文本日志。

SVN 子菜单支持更新、添加、提交、锁定、解锁、显示差异、显示日志、还原和清理工作副本。
SVN 提交提交所选路径的本地更改。
“显示日志”打开 TortoiseSVN 日志窗口。

提交操作要求输入提交说明。还原操作要求确认。
命令在后台任务中执行。旧文件状态不会阻塞界面。

### 右键菜单架构

Kite 使用统一的右键菜单模型。
列表视图和分栏视图使用同一套命令。
菜单动作携带目标路径和工作目录。
新建和粘贴作用于工作目录。
重命名、删除和版本控制操作作用于目标路径。
因此，旧选择不会误用在新菜单动作上。

“Windows 原生菜单…”调用 Windows Shell 菜单。
Windows Shell 菜单包含系统功能和已安装扩展。
这些功能包括“打开方式”“属性”“发送到”、压缩工具和同步工具。
TortoiseGit 和 TortoiseSVN 扩展也可出现在此菜单中。

菜单模型按功能提供者构建。
当前提供者包括核心文件操作、Git/SVN、Windows Shell 和刷新。
详细设计见 `docs/architecture/context-menu.md`。
跨平台边界见 `docs/architecture/platforms.md`。

## 分栏视图与路径栏

Kite 默认使用分栏视图。每一栏显示一级文件夹的内容。
选中文件夹后，Kite 在右侧加载下一栏。

Kite 不设置固定栏数上限。
窗口宽度决定同时可见的栏数。
窗口变窄时，深层栏目保留在横向滚动区域中。
使用触控板横向滚动或拖动分栏底部滚动条即可查看。
分栏区域不锁定滚动轴。
纵向滚轮先滚动指针所在目录。
目录到达顶部或底部后，剩余滚动交给分栏区域。
横向手势始终滚动分栏区域。
短目录和空目录不阻挡分栏滚动。

路径栏显示当前项目的完整路径。
点击任意文件夹段可返回对应层级。
点击路径栏时，Kite 复用已加载的上级栏目。未变化的栏目不重新加载。

Windows 使用 `Alt+P` 切换路径栏。
该快捷键对应 macOS 的 Option 组合键。

列表视图在“版本控制”列显示 Git/SVN 状态。
分栏视图在文件或文件夹图标上显示 Git/SVN 状态码。
Git 状态使用圆形徽标。
SVN 状态使用圆角方形徽标。

## 预览与属性

在分栏视图任意栏选中文件后，Kite 收起右侧分栏并显示预览和属性栏。
属性包含名称、类型、大小、修改日期、位置和 Git/SVN 状态。
Windows 支持图片预览和文本摘要。
Windows 对日志显示末尾预览，不受常规文本文件大小上限影响。
日志预览读取末尾 64 KiB，并显示最后 6,000 个字符。
Windows 对没有内置预览的文件显示类型图标。
macOS 使用 Quick Look 显示原生预览。
预览中的文本内容支持鼠标选取。
预览属性值支持鼠标选取。
Windows 使用 `Ctrl+C` 复制选取内容。
macOS 使用 `Command+C` 复制选取内容。
预览标题栏的复制按钮复制全部属性。
切换目录后预览栏自动关闭。

## 路径导航

路径输入栏是唯一的路径跳转入口。
输入路径后按 Enter 打开目标文件夹。
`Ctrl+L` 聚焦路径输入栏。

工具栏的上一级按钮是唯一的父目录入口。
`Alt+Up` 执行同一动作。

菜单栏的“导航”菜单只提供“聚焦路径栏”。
工具栏不显示重复的“前往”下拉菜单。
路径输入支持绝对路径和相对路径。
`~` 表示个人文件夹。
`/` 表示当前磁盘根目录。

macOS 使用 `Command+L` 聚焦路径栏。
macOS 使用 `Command+Up` 返回父目录。

## 多页签

Kite 使用一个窗口承载多个页签。
每个页签保留独立的路径、历史、分栏、搜索和视图状态。
文件剪贴板由同一窗口的全部页签共享。

新建页签打开当前页签的文件夹。
最后一个页签不能关闭。

页签栏右侧的 `+` 按钮新建页签。
页签的 `×` 按钮关闭页签。
### 会话恢复

Kite 在状态变化和窗口关闭时保存会话。
会话包含页签顺序、活动页签和每个页签的当前目录。
会话还包含列表或分栏模式以及路径栏开关。
应用下次启动时恢复这些页签和目录。

Windows 使用命令行路径启动时，命令行路径优先。
macOS 使用 `Command+T` 新建页签。
macOS 使用 `Command+W` 关闭页签。
macOS 使用 `Control+Tab` 切换页签。

会话文件位于平台本地数据目录。
Windows 路径为 `%LOCALAPPDATA%\Kite\session.json`。
macOS 路径为 `~/Library/Application Support/Kite/session.json`。
Kite 保留 `.bak` 备份。主文件损坏时，Kite 读取备份。
Kite 跳过不存在或不可访问的目录。
搜索词、滚动位置、文件选择和窗口几何尺寸不保存。

## 设备识别

Kite 在侧边栏显示已连接的存储设备和便携设备。
USB 存储设备通过存储总线类型识别。
可移动磁盘显示卷名称、盘符和总容量。
设备列表每 2 秒自动刷新。

USB 存储设备可直接浏览。
点击 WPD/MTP 便携设备可在 Windows Shell 中打开设备。
Kite 内置的 WPD/MTP 文件浏览尚未支持。

## 快捷键

| 快捷键 | 操作 |
| --- | --- |
| `Alt+Left` | 返回 |
| `Alt+Right` | 前进 |
| `Alt+Up` | 上一级 |
| `Ctrl+L` | 聚焦地址栏 |
| `Ctrl+F` | 聚焦搜索框 |
| `Ctrl+1` | 切换到列表视图 |
| `Ctrl+2` | 切换到分栏视图 |
| `Ctrl+T` | 新建页签 |
| `Ctrl+W` | 关闭页签 |
| `Ctrl+Tab` | 下一个页签 |
| `Ctrl+Shift+Tab` | 上一个页签 |
| `Alt+P` | 显示或隐藏路径栏 |
| `Enter` | 打开选中项目 |
| `F2` | 重命名选中项目 |
| `Delete` | 将选中项目移到回收站 |
| `Ctrl+C` | 复制选中项目 |
| `Ctrl+X` | 剪切选中项目 |
| `Ctrl+V` | 粘贴 |
| `Ctrl+Shift+N` | 新建文件夹 |
| `Ctrl+R` | 刷新 |
| `Esc` | 关闭弹窗或清除搜索 |

## 性能设计

- 文件夹读取在后台任务中执行。界面线程不等待磁盘响应。
- 分栏导航复用已加载的公共上级栏目。
- 进入下一层时，上级栏目不进入加载状态。
- 跳转其他分支时，只加载公共前缀之后的栏目。
- 导航请求带有代号。旧请求的结果不会覆盖新文件夹。
- 搜索使用可取消的后台任务。搜索有 1000 个结果上限。
- 文件表格只绘制可见行。大文件夹不会创建同等数量的界面元素。
- 搜索和导航共享排序规则。排序不触发磁盘读取。
- 文件操作在后台任务中执行。界面保持响应。

## 稳定性设计

- 读取失败时保留原文件夹内容。
- 不可读取的项目会计入跳过数量。单个坏项目不终止文件夹读取。
- 隐藏的系统联接点不会显示为普通文件夹。
- 受保护文件夹显示明确权限提示，不显示原始 Windows 错误码。
- 复制冲突自动生成“ - 副本”名称。复制操作不覆盖已有项目。
- 文件夹不能复制到自身内部。
- 删除操作使用 Windows 回收站。
- 删除前显示确认提示。
- 操作错误显示在状态栏。
- 文件层包含路径、排序、搜索、格式化和复制测试。

## 图标设计

图标使用浅蓝天空色玻璃底板。
风筝使用半透明翼面、柔和高光、蓝色阴影和珠光风筝线。
图标没有黑色背景。

图标母版为 `assets/kite-icon-master.png`。
ICO 包含 16、24、32、48、64、128 和 256 像素尺寸。

重新生成图标：

```powershell
py -m pip install pillow
py .\assets\generate_icon.py
.\build.ps1
```

## 当前限制

- 文件表格当前支持单选。
- 复制和剪切使用 Kite 内部剪贴板。内部剪贴板在同一窗口的页签之间共享。Kite 当前不与 Windows 资源管理器共享文件剪贴板。
- 复制操作不支持符号链接。
- 搜索只匹配项目名称。
- 搜索显示前 1000 个匹配项目。
- 表格空白区域尚未提供右键菜单。
- 当前版本没有自定义列布局。
- Git/SVN 状态只覆盖当前视图和当前视图的子项目。
- WPD/MTP 便携设备可在 Windows 中打开，但 Kite 内置文件浏览尚未支持。
- Git 和 SVN 认证使用本机凭据缓存，不显示交互式登录窗口。

## 构建

### Windows 环境

1. 安装稳定版 Rust。
2. 安装 Visual Studio 2022 C++ 生成工具。
3. 安装 Windows 10 或 Windows 11 SDK。
4. 确认 `cargo` 和 `rustc` 在 `PATH` 中。

### Windows 开发运行

```powershell
cargo run --features windows-ui
```

Kite 接受一个可选的启动路径：

```powershell
cargo run --features windows-ui -- "C:\Work\Project"
```

启动路径可以是文件夹或文件。
文件路径会打开文件所在文件夹，并调用系统默认程序打开文件。

### Windows 发布构建

```powershell
.\build.ps1
```

`build.ps1` 生成 `dist\Kite.exe` 和 `dist\Kite-v0.6.14-windows-x64.zip`。

### macOS 构建

macOS 需要稳定版 Rust、Xcode 26 或更新版本和 Xcode Command Line Tools。
部署目标为 macOS 13。
Liquid Glass 在 macOS 26 或更新系统显示。

```bash
bash apps/macos/build.sh
```

脚本生成 `apps/macos/build/Kite.app`。
脚本同时生成 UniFFI Swift 绑定。

### 测试

```powershell
cargo test --lib
cargo test --features windows-ui
cargo clippy --all-targets --features windows-ui -- -D warnings
```

## 项目结构

- `src/app.rs`：Windows GPUI 页签内容、导航状态、剪贴板、文件操作和界面。
- `src/lib.rs`：跨平台 Rust Core 入口和 UniFFI 基础。
- `src/updater.rs`：GitHub Release 检查、版本比较、资产下载和 SHA-256 校验。
- `src/ffi.rs`：Swift 可调用的类型化 Core 接口。
- `src/context_menu/mod.rs`：Windows 菜单模型、提供者和 GPUI 渲染器。
- `src/context_menu/windows.rs`：Windows Shell 原生菜单桥接。
- `src/devices.rs`：Windows USB/WPD 和 macOS 卷枚举。
- `src/tabs.rs`：Windows 页签栏、页签切换和共享剪贴板。
- `src/fs.rs`：跨平台文件夹读取、搜索、排序、格式化和文件操作。
- `src/platform/mod.rs`：跨平台路径和文件名规则。
- `src/session.rs`：跨平台会话保存、恢复和损坏回退。
- `src/table.rs`：Windows 虚拟文件表格和列定义。
- `src/main.rs`：Windows GPUI 前端入口。
- `apps/macos/Sources`：SwiftUI 和 AppKit macOS 前端。
- `build.rs`：Windows 图标和版本资源。
- `assets/kite.ico`：Windows 应用图标。

## 技术栈

- Rust 2024 Edition
- 共享 Rust Core
- UniFFI `0.32`
- Windows：GPUI `0.2.2`、GPUI Kit `0.8.0`、Windows API
- macOS：SwiftUI、AppKit、Liquid Glass、Quick Look、UniFFI Swift
- WalkDir
- trash

## 许可

MIT
