# 发布对齐检查

## 结论

Kite 使用两个自动检查防止 Windows 功能遗漏 macOS 对齐：

- PR 对齐检查。
- 发布对齐检查。

## 标签

- platform:windows：改动影响 Windows 前端或系统集成。
- platform:macos：改动影响 macOS 前端或系统集成。
- parity:required：Windows 改动需要 macOS 对齐评审或后续实现。

## PR 检查

.github/workflows/platform-parity.yml 检查每个 PR。

规则：

1. Windows UI 文件变化需要 platform:windows 标签。
2. macOS 源文件变化需要 platform:macos 标签。
3. Windows UI 变化没有 macOS 源文件变化时需要 parity:required 标签。

## 发布检查

.github/workflows/release-parity.yml 检查版本标签或手动发布。

规则：

1. 比较当前版本和上一个版本。
2. Windows UI 有变化时，要求 macOS 源文件也有变化。
3. 确认发布说明包含 Windows 和 macOS 安装入口。
4. 有打开的 parity:required PR 时停止发布。

## 例外

平台专用功能可以在发布说明写入以下句子：

macOS parity: not required

发布检查只在 Windows UI 有变化且 macOS 源文件没有变化时读取此例外。
