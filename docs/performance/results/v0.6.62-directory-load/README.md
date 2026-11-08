# Directory loading optimization evidence

本目录保存目录读取优化的同机 A/B 数据。

| 目录 | 内容 |
| --- | --- |
| `raw-runs.tar.gz` | 优化前后六次完整原始运行 |
| `raw-runs.tar.gz.sha256` | 原始运行归档校验值 |
| `control-aggregate.json` | 优化前三运行聚合 |
| `optimized-aggregate.json` | 优化后三运行聚合 |
| `comparison.md` | 场景级比较 |

方法：

- 平台：macOS arm64。
- 设备：Generic Apple Silicon，not recorded。
- 配置：`standard`。
- 每场景交互样本：30。
- 每场景冷样本：10。
- 优化前 commit：`e7c8b47cdc149d997e08f4ef4ee9d44f7dfa5976`。
- 优化后 code commit：`7eda754657f65aa1041d10fea726885992f9145d`。
- 两个版本各运行 3 次。
- 比较使用三次运行中位数和噪声阈值。
