# Kite 图标系统

图标名称为 Twin Flow。
图标使用透明画布。
风筝由一块不对称三角翼和两条尾流组成。

## 视觉规则

- 左侧翼面使用氧化橙玻璃。
- 右侧翼面使用藏青玻璃。
- 中央折缝从风筝鼻尖延伸到尾轴。
- 两条尾流分别使用橙色和藏青色。
- 六角尾轴使用藏青色。
- 图标没有方形底板。
- 图标没有旧方案的十字龙骨。
- 图标没有中心齿轮。

## Rust 关联

氧化橙表达 Rust 的名称和材料属性。
藏青色表达稳定结构和安全边界。
六角尾轴表达工程连接。
两条尾流表达并行数据流。
图标不是 Rust 官方标志。

## 资产管线

- `assets/icon/kite-icon.svg`：矢量母版。
- `assets/kite-icon-master.png`：1024 像素透明 PNG。
- `assets/kite.ico`：Windows 图标。
- `assets/kite.icns`：macOS 图标。
- `assets/kite-icon-preview.png`：浅色、深色和尺寸预览。

## 生成要求

1. 安装 Python 和 Pillow。
2. 安装 Chrome、Edge 或 Chromium。
3. 运行 `py .\assets\icon\build_icons.py`。
4. 检查 `assets/kite-icon-preview.png`。

脚本用浏览器把 SVG 渲染成透明 PNG。
脚本用 Pillow 生成 ICO、ICNS 和预览板。
