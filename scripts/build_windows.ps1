# swCutter 一键构建脚本（Windows）
# 用法: powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1 [-Release]
param(
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# 工具链环境（本机安装位置）
if (Test-Path 'H:\dev\flutter\bin') {
    $env:PATH = "H:\dev\flutter\bin;$env:PATH"
}
$env:PUB_HOSTED_URL = 'https://pub.flutter-io.cn'
$env:FLUTTER_STORAGE_BASE_URL = 'https://mirrors.cloud.tencent.com/flutter'

Set-Location $root

Write-Host '== [1/3] Rust 单元测试 ==' -ForegroundColor Cyan
Push-Location rust
cargo test --quiet
Pop-Location

Write-Host '== [2/3] Flutter 分析 ==' -ForegroundColor Cyan
flutter analyze

Write-Host '== [3/3] 构建 Windows 应用 ==' -ForegroundColor Cyan
if ($Release) {
    flutter build windows --release
} else {
    flutter build windows --debug
}

$mode = if ($Release) { 'Release' } else { 'Debug' }
Write-Host "`n✅ 构建完成 ($mode)" -ForegroundColor Green
Write-Host "   产物: $root\build\windows\x64\runner\$mode\sw_cutter.exe"
