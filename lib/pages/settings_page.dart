import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state/app_state.dart';

class SettingsPage extends ConsumerWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final app = ref.watch(appProvider);
    final cs = Theme.of(context).colorScheme;

    return ListView(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 16),
      children: [
        Text('设置',
            style: Theme.of(context)
                .textTheme
                .headlineSmall
                ?.copyWith(fontWeight: FontWeight.w700)),
        const SizedBox(height: 14),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Icon(Icons.bolt_rounded, size: 17, color: cs.primary),
                  const SizedBox(width: 6),
                  const Text('并行任务数',
                      style:
                          TextStyle(fontWeight: FontWeight.w700, fontSize: 14)),
                  const Spacer(),
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
                    decoration: BoxDecoration(
                      color: cs.primary.withValues(alpha: 0.13),
                      borderRadius: BorderRadius.circular(999),
                    ),
                    child: Text('${app.concurrency}',
                        style: TextStyle(
                            fontWeight: FontWeight.w800, color: cs.primary)),
                  ),
                ]),
                Slider(
                  value: app.concurrency.toDouble(),
                  min: 1,
                  max: 8,
                  divisions: 7,
                  label: '${app.concurrency}',
                  onChanged: (v) => ref
                      .read(appProvider)
                      .setConcurrency(v.round()),
                ),
                Text('同时切片的任务数量。每个任务内部还会按 CPU 核数并行渲染瓦片。',
                    style: TextStyle(fontSize: 12, color: cs.outline)),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Icon(Icons.dark_mode_outlined, size: 17, color: cs.primary),
                  const SizedBox(width: 6),
                  const Text('外观',
                      style:
                          TextStyle(fontWeight: FontWeight.w700, fontSize: 14)),
                ]),
                const SizedBox(height: 8),
                SegmentedButton<ThemeMode>(
                  segments: const [
                    ButtonSegment(value: ThemeMode.dark, label: Text('深色'), icon: Icon(Icons.nightlight_round, size: 15)),
                    ButtonSegment(value: ThemeMode.light, label: Text('浅色'), icon: Icon(Icons.wb_sunny_rounded, size: 15)),
                    ButtonSegment(value: ThemeMode.system, label: Text('跟随系统')),
                  ],
                  selected: {app.themeMode},
                  onSelectionChanged: (s) =>
                      ref.read(appProvider).setThemeMode(s.first),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Icon(Icons.folder_special_rounded, size: 17, color: cs.primary),
                  const SizedBox(width: 6),
                  const Text('默认输出目录',
                      style:
                          TextStyle(fontWeight: FontWeight.w700, fontSize: 14)),
                ]),
                const SizedBox(height: 8),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        app.defaultOutput.isEmpty ? '未设置（使用源文件旁的 *_tiles 目录）' : app.defaultOutput,
                        style: TextStyle(
                            fontSize: 12.5,
                            color: app.defaultOutput.isEmpty
                                ? cs.outline
                                : cs.onSurface),
                      ),
                    ),
                    OutlinedButton(
                      onPressed: () async {
                        final d = await FilePicker.getDirectoryPath(
                            dialogTitle: '选择默认输出目录');
                        if (d != null) ref.read(appProvider).setDefaultOutput(d);
                      },
                      child: const Text('选择'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Row(
              children: [
                Icon(Icons.info_outline_rounded, size: 17, color: cs.primary),
                const SizedBox(width: 8),
                const Expanded(
                  child: Text(
                    'swCutter v0.1 — Flutter + Rust TIFF 金字塔切片工具。'
                    '支持 XYZ / TMS 目录、PNG 输出、颜色键透明与浏览器瓦片预览。',
                    style: TextStyle(fontSize: 12.5),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
