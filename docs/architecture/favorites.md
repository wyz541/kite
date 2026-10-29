# Kite 收藏架构

## 结论

收藏由共享 Rust Core 保存。
Windows 和 macOS 使用同一个 JSON schema。
收藏栏只保存路径、显示名和文件/文件夹类型。

## 数据位置

默认配置文件为 `Kite/favorites.json`。

- Windows 使用 `%APPDATA%\Kite\favorites.json`。
- macOS 使用 `~/Library/Application Support/Kite/favorites.json`。
- `KITE_FAVORITES_PATH` 可以覆盖路径，用于测试和诊断。

## 数据格式

```json
{
  "schema_version": 1,
  "entries": [
    {
      "path": "C:\\Work\\Project",
      "name": "Project",
      "is_directory": true
    }
  ]
}
```

## 行为

- Windows 右键文件或文件夹时显示“添加到收藏”。
- macOS 右键文件、文件夹或目录区域时显示“添加到收藏”。
- 重复路径不新增第二项。
- 收藏文件夹点击后导航到该文件夹。
- 收藏文件点击后使用系统默认程序打开。
- Windows 收藏项右键显示“从收藏移除”。
- macOS 收藏胶囊右键显示“从收藏移除”。

## 稳定性

- 收藏最多保存 128 项。
- 配置大小限制为 512 KiB。
- 路径使用平台路径规则去重。
- 写入使用临时文件和原子替换。
- 主文件损坏时读取备份文件。
- 损坏配置回退为空收藏。
- 收藏不阻止应用启动。

## 验证

- 添加收藏测试。
- 重复收藏测试。
- 移除收藏测试。
- 备份读取测试由持久化实现覆盖。
- Windows GPUI 编译测试。
- macOS SwiftUI 编译测试。
