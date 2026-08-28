import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state/app_state.dart';
import '../src/rust/engine/planner.dart';
import '../version.dart';

/// 底图预设（XYZ 模板；天地图模板含 {tk} 占位，生成预览时注入密钥）
const List<Map<String, String>> kBasemapPresets = [
  {
    'name': 'OpenStreetMap',
    'tpl': 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
  },
  {
    'name': 'ArcGIS·卫星影像',
    'tpl':
        'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}',
  },
  {
    'name': '谷歌·卫星',
    'tpl': 'https://mt{s}.google.com/vt/lyrs=s&x={x}&y={y}&z={z}',
    'subs': '0123',
  },
  {
    'name': '谷歌·矢量',
    'tpl': 'https://mt{s}.google.com/vt/lyrs=m&x={x}&y={y}&z={z}',
    'subs': '0123',
  },
  {
    'name': '高德·矢量',
    'tpl':
        'https://webrd0{s}.is.autonavi.com/appmaptile?lang=zh_cn&size=1&scale=1&style=8&x={x}&y={y}&z={z}',
    'subs': '1234',
  },
  {
    'name': '天地图·矢量',
    'tpl': 'https://t{s}.tianditu.gov.cn/DataServer?T=vec_w&x={x}&y={y}&l={z}&tk={tk}',
    'subs': '01234567',
  },
  {
    'name': '天地图·影像',
    'tpl': 'https://t{s}.tianditu.gov.cn/DataServer?T=img_w&x={x}&y={y}&l={z}&tk={tk}',
    'subs': '01234567',
  },
];

class SettingsPage extends ConsumerStatefulWidget {
  const SettingsPage({super.key});

  @override
  ConsumerState<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends ConsumerState<SettingsPage> {
  @override
  Widget build(BuildContext context) {
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
                  onChanged: (v) =>
                      ref.read(appProvider).setConcurrency(v.round()),
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
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Icon(Icons.tune_rounded, size: 17, color: cs.primary),
                  const SizedBox(width: 6),
                  const Text('切片默认参数（全局）',
                      style:
                          TextStyle(fontWeight: FontWeight.w700, fontSize: 14)),
                ]),
                const SizedBox(height: 10),
                Row(
                  children: [
                    const Text('瓦片尺寸',
                        style: TextStyle(fontSize: 13)),
                    const Spacer(),
                    SegmentedButton<int>(
                      segments: const [
                        ButtonSegment(value: 256, label: Text('256')),
                        ButtonSegment(value: 512, label: Text('512')),
                      ],
                      selected: {app.tileSize},
                      onSelectionChanged: (s) =>
                          ref.read(appProvider).setTileSize(s.first),
                    ),
                  ],
                ),
                SwitchListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  title: const Text('跳过全透明瓦片',
                      style: TextStyle(fontSize: 13)),
                  subtitle: const Text(
                      '关闭时与 gdal2tiles 一致：空白区域也输出透明 PNG（默认关闭）',
                      style: TextStyle(fontSize: 11)),
                  value: app.skipEmpty,
                  onChanged: (v) =>
                      ref.read(appProvider).setSkipEmpty(v),
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    const Text('重采样', style: TextStyle(fontSize: 13)),
                    const Spacer(),
                    SegmentedButton<Resample>(
                      segments: const [
                        ButtonSegment(value: Resample.nearest, label: Text('最近邻')),
                        ButtonSegment(value: Resample.bilinear, label: Text('双线性')),
                      ],
                      selected: {app.resample},
                      onSelectionChanged: (s) =>
                          ref.read(appProvider).setResample(s.first),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),

        // ---- 预览底图 ----
        Card(
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Icon(Icons.map_rounded, size: 17, color: cs.primary),
                  const SizedBox(width: 8),
                  const Text('预览底图',
                      style: TextStyle(fontSize: 14.5, fontWeight: FontWeight.w700)),
                ]),
                const SizedBox(height: 6),
                Text('写入每个 preview.html 作为默认底图/叠加层。'
                    '点击预设即添加（再次点击移除，已添加显示 ✓）。'
                    '公开免密钥：OpenStreetMap（矢量，默认启用）、ArcGIS World Imagery（卫星）；'
                    '谷歌无需密钥但请注意其服务条款；需要密钥的服务（如天地图）统一在下方「密钥设置」中维护。',
                    style: TextStyle(fontSize: 11, color: Colors.grey.shade500)),
                const SizedBox(height: 10),
                Wrap(spacing: 8, runSpacing: 8, children: [
                  for (final e in kBasemapPresets)
                    ActionChip(
                      label: Text(e['name']!, style: const TextStyle(fontSize: 11.5)),
                      tooltip: e['tpl'],
                      avatar: app.baseMaps.any((m) => m['name'] == e['name'])
                          ? const Icon(Icons.check_rounded, size: 15)
                          : null,
                      backgroundColor:
                          app.baseMaps.any((m) => m['name'] == e['name'])
                              ? cs.primary.withValues(alpha: 0.15)
                              : null,
                      onPressed: () {
                        final a = ref.read(appProvider);
                        final ex =
                            a.baseMaps.indexWhere((m) => m['name'] == e['name']);
                        if (ex >= 0) {
                          a.removeBasemap(ex);
                        } else {
                          a.addBasemap({
                            ...e,
                            'on': true, 'below': true, 'opacity': 1.0,
                            'zmin': 2, 'zmax': 19,
                          });
                        }
                      },
                    ),
                ]),
                const SizedBox(height: 10),
                Row(children: [
                  FilledButton.tonalIcon(
                    onPressed: _showKeysDialog,
                    icon: const Icon(Icons.key_rounded, size: 17),
                    label: const Text('密钥设置',
                        style: TextStyle(fontSize: 12.5)),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(
                      _keysSummary(app),
                      style: TextStyle(fontSize: 11.5, color: cs.outline),
                    ),
                  ),
                ]),
                const SizedBox(height: 12),
                for (var i = 0; i < app.baseMaps.length; i++)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: Row(children: [
                      Checkbox(
                        value: app.baseMaps[i]['on'] == true,
                        onChanged: (v) =>
                            ref.read(appProvider).setBasemapOn(i, v ?? false),
                      ),
                      Expanded(
                        child: Text('${app.baseMaps[i]['name']}',
                            style: const TextStyle(fontSize: 12.5)),
                      ),
                      SegmentedButton<int>(
                        style: ButtonStyle(
                          visualDensity: VisualDensity.compact,
                          padding: const WidgetStatePropertyAll(
                              EdgeInsets.symmetric(horizontal: 8)),
                          textStyle: const WidgetStatePropertyAll(
                              TextStyle(fontSize: 11)),
                        ),
                        segments: const [
                          ButtonSegment(value: 0, label: Text('底图')),
                          ButtonSegment(value: 1, label: Text('叠加')),
                        ],
                        selected: {app.baseMaps[i]['below'] != false ? 0 : 1},
                        showSelectedIcon: false,
                        onSelectionChanged: (s) => ref
                            .read(appProvider)
                            .setBasemapBelow(i, s.first == 0),
                      ),
                      const SizedBox(width: 4),
                      IconButton(
                        tooltip: '删除',
                        icon:
                            const Icon(Icons.delete_outline_rounded, size: 18),
                        onPressed: () =>
                            ref.read(appProvider).removeBasemap(i),
                      ),
                    ]),
                  ),
                if (app.baseMaps.isEmpty)
                  Text('尚未添加任何底图/叠加层（预览页将只有切片影像本身）',
                      style: TextStyle(fontSize: 11.5, color: cs.outline)),
              ]),
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
                Expanded(
                  child: Text(
                    'swCutter v$kAppVersion — Flutter + Rust TIFF 金字塔切片工具。'
                    'GDAL 绝对级别 · XYZ/TMS 目录 · PNG 输出 · 颜色键透明与浏览器瓦片预览。',
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

  /// 收集所有底图模板中的密钥占位符（排除瓦片坐标 {z}/{x}/{y} 与子域 {s}）。
  Set<String> _keyPlaceholders() {
    final keys = <String>{};
    final app = ref.read(appProvider);
    for (final e in [...kBasemapPresets, ...app.baseMaps]) {
      final tpl = (e['tpl'] as String?) ?? '';
      for (final m in RegExp(r'\{([a-zA-Z0-9]+)\}').allMatches(tpl)) {
        final name = m.group(1)!;
        if (!{'z', 'x', 'y', 's'}.contains(name)) keys.add(name);
      }
    }
    return keys;
  }

  String _keyLabel(String name) => switch (name) {
        'tk' => '天地图 tk 密钥',
        _ => '密钥：$name',
      };

  /// 使用该密钥的服务名称列表。
  List<String> _keyUsers(String name) {
    final users = <String>[];
    for (final e in kBasemapPresets) {
      final tpl = e['tpl'] ?? '';
      if (tpl.contains('{$name}')) users.add(e['name']!);
    }
    return users;
  }

  String _keysSummary(AppState app) {
    final keys = _keyPlaceholders();
    if (keys.isEmpty) return '当前底图模板不需要密钥';
    final parts = keys.map((k) {
      final v = app.mapKeys[k] ?? '';
      final set = v.isNotEmpty;
      return '$k：${set ? '已设置' : '未设置'}';
    });
    return '需要密钥的服务：${parts.join('，')}';
  }

  /// 密钥设置弹窗：按模板占位符动态列出所有需要密钥的服务，统一在一个弹窗维护。
  Future<void> _showKeysDialog() async {
    final app = ref.read(appProvider);
    final keys = _keyPlaceholders().toList()..sort();
    if (keys.isEmpty) return;
    final controllers = {
      for (final k in keys) k: TextEditingController(text: app.mapKeys[k] ?? ''),
    };
    final saved = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('地图服务密钥'),
        content: SizedBox(
          width: 400,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final k in keys) ...[
                  TextField(
                    controller: controllers[k],
                    style: const TextStyle(fontSize: 12.5),
                    decoration: InputDecoration(
                      labelText: _keyLabel(k),
                      hintText: '用于：${_keyUsers(k).join('、')}',
                      isDense: true,
                      border: const OutlineInputBorder(),
                    ),
                  ),
                  const SizedBox(height: 12),
                ],
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('保存'),
          ),
        ],
      ),
    );
    // 先取值，再释放控制器（dispose 后不可再读 text）
    final values = {for (final k in keys) k: controllers[k]!.text.trim()};
    for (final k in keys) {
      controllers[k]!.dispose();
    }
    if (saved == true) {
      for (final k in keys) {
        ref.read(appProvider).setMapKey(k, values[k]!);
      }
    }
  }
}
