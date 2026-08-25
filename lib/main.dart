import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app_theme.dart';
import 'pages/home_shell.dart';
import 'state/app_state.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final state = AppState();
  // 先加载设置再挂载 UI（主题即时生效）
  await state.loadSettings();
  runApp(ProviderScope(
    overrides: [appProvider.overrideWith((ref) => state)],
    child: const SwCutterApp(),
  ));
  // 后台继续：拉历史任务、订阅事件
  unawaited(state.bootstrap());
}

class SwCutterApp extends ConsumerWidget {
  const SwCutterApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appProvider);
    return MaterialApp(
      title: 'swCutter · TIFF 金字塔切片',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      themeMode: state.themeMode,
      home: const HomeShell(),
    );
  }
}
