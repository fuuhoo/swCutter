# swCutter 一键构建脚本 — Debug 版
#
# 用法:
#   powershell -ExecutionPolicy Bypass -File scripts\build_debug.ps1
#
# 工作流程:
#   1. cargo test                              → Rust 单元测试（含 preview.html z-order 回归）
#   2. flutter pub get                         → 拉取 Dart 依赖
#   3. flutter analyze                         → Dart 静态检查
#   4. flutter build windows --debug           → 编译应用（含 Rust native 二进制）
#
# 产物:
#   H:\project_self\swCutter\build\windows\x64\runner\Debug\sw_cutter.exe

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# 工具链路径（本机安装位置）
if (Test-Path 'H:\dev\flutter\bin') {
    $env:PATH = "H:\dev\flutter\bin;$env:PATH"
}
# 镜像加速（国内）
$env:PUB_HOSTED_URL = 'https://pub.flutter-io.cn'
$env:FLUTTER_STORAGE_BASE_URL = 'https://mirrors.cloud.tencent.com/flutter'

Set-Location $root

function Step($n, $title) {
    Write-Host "`n== [$n] $title ==" -ForegroundColor Cyan
}

# ---- [1] Rust 单元测试 ----
Step '1/4' 'Rust 单元测试（含 preview.html z-order 回归）'
Push-Location $root\rust
cargo test --quiet
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "Rust 测试失败（exit $LASTEXITCODE）"
}
Pop-Location
Write-Host '   ✓ Rust 测试通过' -ForegroundColor Green

# ---- [2] flutter pub get ----
Step '2/4' 'flutter pub get'
flutter pub get
if ($LASTEXITCODE -ne 0) {
    throw "flutter pub get 失败（exit $LASTEXITCODE）"
}

# ---- [3] flutter analyze ----
Step '3/4' 'flutter analyze（静态检查）'
flutter analyze
if ($LASTEXITCODE -ne 0) {
    throw "flutter analyze 失败（exit $LASTEXITCODE）"
}
Write-Host '   ✓ 静态检查通过' -ForegroundColor Green

# ---- [4] flutter build windows --debug ----
Step '4/4' 'flutter build windows --debug'
Write-Host "   构建模式: debug（开发用，含调试信息）" -ForegroundColor Yellow
flutter build windows --debug
if ($LASTEXITCODE -ne 0) {
    throw "flutter build 失败（exit $LASTEXITCODE）"
}

Write-Host "`n✅ Debug 构建完成" -ForegroundColor Green
Write-Host "   产物: $root\build\windows\x64\runner\Debug\sw_cutter.exe" -ForegroundColor Green
Write-Host "`n👉 运行新构建以验证修复：" -ForegroundColor Cyan
Write-Host "   explorer `"$root\build\windows\x64\runner\Debug`"" -ForegroundColor Gray