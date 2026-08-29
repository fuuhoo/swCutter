#!/usr/bin/env bash
#
# swCutter — macOS 一键构建脚本
#
# 用法:
#   bash scripts/build_macos.sh            # Debug
#   bash scripts/build_macos.sh --release  # Release
#
# 前置条件:
#   - Flutter SDK 3.13+ 已装入 PATH (含 macOS 桌面支持: flutter config --enable-macos-desktop)
#   - 已安装 Xcode (App Store 安装)
#   - 已安装 CocoaPods (`sudo gem install cocoapods` 或 `brew install cocoapods`)
#
# 可选便携工具链:
#   若本仓库下存在 `.tools/`（gitignore 由仓库管理），脚本会自动 source
#   `.tools/activate.sh`，把仓库内便携 Rust/Flutter 安装到当前 shell 的 PATH
#   并启用国内镜像（无需 sudo，适合沙盒化 macOS 用户级账户）。
#
# 工作流程:
#   1. cargo test --release                     → Rust 单元测试
#   2. flutter pub get                          → 拉取 Dart 依赖
#   3. flutter analyze                          → Dart 静态检查
#   4. flutter build macos [--release]          → 构建应用
#
# 产物:
#   Debug:   build/macos/Build/Products/Debug/sw_cutter.app
#   Release: build/macos/Build/Products/Release/sw_cutter.app
#
# 首次构建时间较长（10-25 分钟），cargokit 需现编译 Rust 静态库。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# 自动激活便携工具链（Rust + Flutter + 国内镜像），覆盖 PATH / HOME / GEM_*
# 已经激活过（即 DSH_TOOLS_ACTIVATED 已存在）则跳过。
if [[ -z "${DSH_TOOLS_ACTIVATED:-}" && -f "$ROOT_DIR/.tools/activate.sh" ]]; then
  # shellcheck disable=SC1091
  source "$ROOT_DIR/.tools/activate.sh"
fi

BUILD_MODE="debug"
case "${1:-}" in
  --release|release|Release|-Release)
    BUILD_MODE="release"
    ;;
  --debug|debug|Debug|"")
    BUILD_MODE="debug"
    ;;
  *)
    echo "未知参数: $1 (支持: --release | --debug)" >&2
    exit 2
    ;;
esac

# --- 颜色 (仅当 stdout 是 TTY 时) ---
if [[ -t 1 ]]; then
  C_CYAN='\033[0;36m'
  C_GREEN='\033[0;32m'
  C_YELLOW='\033[0;33m'
  C_RESET='\033[0m'
else
  C_CYAN=''; C_GREEN=''; C_YELLOW=''; C_RESET=''
fi

step() { echo -e "\n== [$1] $2 ==${C_CYAN}"; }

# --- 工具检查 ---
command -v flutter >/dev/null 2>&1 || { echo "错误: 未找到 flutter，请先安装 Flutter SDK 并加入 PATH" >&2; exit 1; }
command -v cargo    >/dev/null 2>&1 || { echo "错误: 未找到 cargo，请先安装 Rust 工具链" >&2; exit 1; }
command -v pod      >/dev/null 2>&1 || { echo "警告: 未找到 cocoapods；flutter build macos 会自动调用，如未装请先 gem/brew 安装 cocoapods" >&2; }
xcode-select -p >/dev/null 2>&1 || { echo "错误: 未配置 CommandLineTools / Xcode，执行 xcode-select --install 或装 Xcode" >&2; exit 1; }

# --- 1. Rust 单元测试 ---
step "1/4" "Rust 单元测试 (profile=${BUILD_MODE})"
(
  cd "${ROOT_DIR}/rust"
  if [[ "$BUILD_MODE" == "release" ]]; then
    cargo test --release --quiet
  else
    cargo test --quiet
  fi
)
echo -e "   ${C_GREEN}✓ Rust 测试通过${C_RESET}"

# --- 2. flutter pub get ---
step "2/4" "flutter pub get"
flutter pub get

# --- 3. flutter analyze ---
step "3/4" "flutter analyze (静态检查)"
flutter analyze
echo -e "   ${C_GREEN}✓ 静态检查通过${C_RESET}"

# --- 4. flutter build macos ---
step "4/4" "flutter build macos (mode=${BUILD_MODE})"
if [[ "$BUILD_MODE" == "release" ]]; then
  echo -e "   ${C_YELLOW}构建模式: release（发布优化，体积小、性能好）${C_RESET}"
  flutter build macos --release
else
  echo -e "   ${C_YELLOW}构建模式: debug${C_RESET}"
  flutter build macos --debug
fi

PRODUCT_DIR="${ROOT_DIR}/build/macos/Build/Products/$(tr '[:lower:]' '[:upper:]' <<< "${BUILD_MODE:0:1}")${BUILD_MODE:1}"
APP_PATH="${PRODUCT_DIR}/sw_cutter.app"

echo
echo -e "${C_GREEN}✅ macOS 构建完成${C_RESET}"
echo -e "   产物: ${APP_PATH}"
echo
echo -e "👉 运行新构建以验证："
echo -e "   open \"${APP_PATH}\""
