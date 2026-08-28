# swCutter 一键构建脚本（Windows）
#
# 此脚本是兼容旧入口的"调度器"——根据参数转发到具体的 build_debug.ps1 或 build_release.ps1。
# 实际构建逻辑在对应脚本中：
#   - scripts\build_debug.ps1
#   - scripts\build_release.ps1
#
# 用法:
#   powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1                 # Debug
#   powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1 -Release        # Release

param(
    [switch]$Release
)

$scriptDir = Split-Path -Parent $PSCommandPath
if ($Release) {
    & "$scriptDir\build_release.ps1"
} else {
    & "$scriptDir\build_debug.ps1"
}