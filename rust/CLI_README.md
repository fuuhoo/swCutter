# swcutter CLI 使用文档

## 概述

`swcutter` 是一个命令行瓦片切片工具，用于将 GeoTIFF 影像切分为 Web 瓦片（PNG 格式），支持 Mercator 投影和相对级别模式。

## 基本用法

```powershell
swcutter.exe --source <源文件> --output <输出目录> [选项]
```

## 参数说明

| 参数 | 简写 | 说明 | 默认值 | 必填 |
|------|------|------|--------|------|
| `--source` | `-s` | 源 GeoTIFF 文件路径 | - | 是 |
| `--output` | `-o` | 输出目录 | - | 是 |
| `--tile-size` | `-t` | 瓦片尺寸（像素） | 256 | 否 |
| `--zmin` | - | 最小缩放级别 | 自动计算 | 否 |
| `--zmax` | - | 最大缩放级别 | 自动计算 | 否 |
| `--scheme` | - | 瓦片编号方案 | xyz | 否 |
| `--alpha` | - | 透明处理模式 | keep | 否 |
| `--resample` | - | 重采样方法 | bilinear | 否 |
| `--skip-empty` | - | 跳过完全透明的瓦片 | false | 否 |
| `--mercator` | - | 使用 Mercator 绝对级别模式 | false | 否 |

## 参数详解

### --scheme 瓦片编号方案

- `xyz`：标准 XYZ 瓦片编号（TMS 翻转 Y 轴）
- `tms`：TMS 瓦片编号（Y 轴从南向北）

### --alpha 透明处理模式

- `keep`：保留源影像 Alpha 通道
- `threshold`：Alpha 阈值模式（低于 128 的像素置为全透明）
- `colorkey`：颜色键模式（接近黑色的像素置为透明，容差 30）

### --resample 重采样方法

- `nearest`：最近邻插值（速度快，锯齿明显）
- `bilinear`：双线性插值（速度适中，效果平滑）

### --mercator Mercator 模式

启用后使用 GDAL 风格的绝对级别系统，需要 GeoTIFF 包含地理参考信息（EPSG:32648 等）。系统会自动计算合适的缩放级别范围。

## 使用示例

### 基本切片

```powershell
swcutter.exe -s "F:\data\input.tif" -o "F:\output\tiles"
```

### 指定级别范围

```powershell
swcutter.exe -s "F:\data\input.tif" -o "F:\output\tiles" --zmin 1 --zmax 18
```

### Mercator 模式切片

```powershell
swcutter.exe -s "F:\data\input.tif" -o "F:\output\tiles" --zmin 1 --zmax 18 --mercator
```

### 自定义瓦片尺寸

```powershell
swcutter.exe -s "F:\data\input.tif" -o "F:\output\tiles" -t 512
```

### 跳过空瓦片

```powershell
swcutter.exe -s "F:\data\input.tif" -o "F:\output\tiles" --skip-empty
```

### 使用颜色键透明

```powershell
swcutter.exe -s "F:\data\input.tif" -o "F:\output\tiles" --alpha colorkey
```

## 测试命令

使用测试数据验证功能：

```powershell
swcutter.exe --source "F:\tiffUpload\siwei\tile_0.TIF" --output "F:\tiffUpload\siwei\tile_0" --zmin 1 --zmax 18 --mercator
```

## 输出结构

切片完成后，输出目录包含：

```
output/
├── manifest.json          # 瓦片元信息
├── preview.html           # MapLibre 预览页面
├── {z}/                   # 缩放级别目录
│   ├── {x}/               # 列目录
│   │   ├── {y}.png        # 瓦片文件
│   │   └── ...
│   └── ...
└── maplibre_assets/       # MapLibre GL JS 资源
    ├── maplibre-gl.js
    └── maplibre-gl.css
```

## 注意事项

1. 源文件必须是有效的 GeoTIFF 格式
2. Mercator 模式需要影像包含地理参考信息
3. 大文件切片可能需要较长时间，建议使用 `--mercator` 模式
4. 切片支持断点续切，中断后重新运行相同参数会跳过已完成的瓦片
