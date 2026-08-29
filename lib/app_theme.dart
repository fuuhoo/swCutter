import 'package:flutter/material.dart';

/// swCutter 现代主题：Material 3，深色为主，柔和圆角与渐变强调。
class AppTheme {
  static const _seed = Color(0xFF4F8CFF); // 科技蓝

  static ThemeData dark() => _base(Brightness.dark);
  static ThemeData light() => _base(Brightness.light);

  static ThemeData _base(Brightness brightness) {
    final scheme = ColorScheme.fromSeed(
      seedColor: _seed,
      brightness: brightness,
    );
    final isDark = brightness == Brightness.dark;
    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor:
          isDark ? const Color(0xFF0F1219) : const Color(0xFFF6F8FC),
      // 跨平台中文 / 拉丁字体回退，避免某些 macOS / Linux 上中文回退到等宽字体。
      // 顺序：macOS/iOS → Linux/跨平台字体 → Windows；缺失时逐级退化。
      fontFamilyFallback: const [
        // macOS / iOS 系统自带中文
        'PingFang SC', 'Heiti SC', 'Songti SC', 'STSong',
        // 跨平台中文（开源，免安装）
        'Noto Sans CJK SC', 'Noto Sans SC', 'Source Han Sans CN',
        // Windows 中文（兼容历史版本）
        'Microsoft YaHei UI', 'Microsoft YaHei', 'SimHei',
        // 拉丁
        'SF Pro Text', 'Helvetica Neue', 'Segoe UI', 'Roboto',
      ],
      cardTheme: CardThemeData(
        elevation: 0,
        color: isDark ? const Color(0xFF171C26) : Colors.white,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(16),
          side: BorderSide(
            color: isDark ? Colors.white.withValues(alpha: 0.06) : Colors.black.withValues(alpha: 0.05),
          ),
        ),
        margin: EdgeInsets.zero,
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: isDark ? Colors.white.withValues(alpha: 0.04) : Colors.black.withValues(alpha: 0.03),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(12),
          borderSide: BorderSide.none,
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(12),
          borderSide: BorderSide(color: scheme.primary, width: 1.5),
        ),
        contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
          padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 14),
          textStyle: const TextStyle(fontWeight: FontWeight.w600),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        ),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
          shape: WidgetStatePropertyAll(
            RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          ),
          visualDensity: VisualDensity.compact,
        ),
      ),
      sliderTheme: const SliderThemeData(showValueIndicator: ShowValueIndicator.onDrag),
      snackBarTheme: SnackBarThemeData(
        behavior: SnackBarBehavior.floating,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
      dividerTheme: DividerThemeData(
        color: isDark ? Colors.white.withValues(alpha: 0.07) : Colors.black.withValues(alpha: 0.08),
        space: 1,
        thickness: 1,
      ),
    );
  }
}
