# swCutter 一键构建脚本 — Release 版
#
# 用法:
#   powershell -ExecutionPolicy Bypass -File scripts\build_release.ps1
#
# 工作流程:
#   1. cargo test --release                     → Rust 单元测试（release profile）
#   2. flutter pub get                         → 拉取 Dart 依赖
#   3. flutter analyze                         → Dart 静态检查
#   4. flutter build windows --release         → 编译发布版应用
#
# 产物:
#   H:\project_self\swCutter\build\windows\x64\runner\Release\sw_cutter.exe
#
# 注意: Release 构建包含 Rust release profile 编译，耗时较长（10-25 分钟）。

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

# ---- [1] Rust 单元测试（release profile） ----
Step '1/4' 'Rust 单元测试（release profile）'
Push-Location $root\rust
cargo test --release --quiet
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

# ---- [4] flutter build windows --release ----
Step '4/4' 'flutter build windows --release'
Write-Host "   构建模式: release（发布优化，体积小、性能好）" -ForegroundColor Yellow
flutter build windows --release
if ($LASTEXITCODE -ne 0) {
    throw "flutter build 失败（exit $LASTEXITCODE）"
}

Write-Host "`n✅ Release 构建完成" -ForegroundColor Green
Write-Host "   产物: $root\build\windows\x64\runner\Release\sw_cutter.exe" -ForegroundColor Green
Write-Host "`n👉 运行新构建以验证修复：" -ForegroundColor Cyan
Write-Host "   explorer `"$root\build\windows\x64\runner\Release`"" -ForegroundColor Gray