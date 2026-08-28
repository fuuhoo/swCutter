import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state/app_state.dart';
import '../src/rust/api/task_api.dart' as rust;
import '../src/rust/engine/alpha.dart';
import '../src/rust/engine/planner.dart';
import '../widgets/task_card.dart' show fmtBytes, schemeName;

/// 新建任务：左侧参数表单 + 右侧预览。
class NewTaskPage extends ConsumerStatefulWidget {
  const NewTaskPage({super.key});

  @override
  ConsumerState<NewTaskPage> createState() => _NewTaskPageState();
}

class _NewTaskPageState extends ConsumerState<NewTaskPage> {
  /// 单文件选择：替换当前草稿（新建任务页始终只处理一个输入）。
  Future<void> _pickSource() async {
    final files = await FilePicker.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['tif', 'tiff'],
    );
    for (final f in files) {
      final p = f.path;
      if (p != null && p.isNotEmpty) {
        await _addSource(p);
        break;
      }
    }
  }

  Future<rust.ImageBrief> _readInfo(String path) => rust.readImageInfo(path: path);

  Future<void> _addSource(String path) async {
    final store = ref.read(draftProvider);
    final app = ref.read(appProvider);
    try {
      final info = await _readInfo(path);
      final draft = TaskDraft(
        source: path,
        fileName: path.split(Platform.pathSeparator).last,
        width: info.width,
        height: info.height,
        maxLevel: info.maxLevel,
      );
      // 仅保留 GDAL 绝对级别模式
      draft.mercator = true;
      // 级别下限 Z1（不生成 Z0）
      draft.zmin = 1;
      // 全局设置同步
      draft.tileSize = app.tileSize;
      draft.skipEmpty = app.skipEmpty;
      draft.resample = app.resample;
      // 输出目录规则：<默认输出>/<tiff名>；未设置默认目录时用源旁 <tiff名>_tiles
      final stem = draft.fileName.contains('.')
          ? draft.fileName.substring(0, draft.fileName.lastIndexOf('.'))
          : draft.fileName;
      final srcDir = Directory(path).parent.path;
      final base = app.defaultOutput.isNotEmpty ? app.defaultOutput : '$srcDir\\';
      draft.outputDir = base.endsWith('\\') || base.endsWith('/')
          ? '$base$stem'
          : '$base${Platform.pathSeparator}$stem';
      store.drafts.clear();
      store.add(draft);
      unawaited(store.loadPreview(draft));
      unawaited(store.refreshEstimates(draft));
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('无法读取 $path\n$e')));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final drafts = ref.watch(draftProvider);
    final active = drafts.active;
    final cs = Theme.of(context).colorScheme;

    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('新建切片任务',
              style: Theme.of(context)
                  .textTheme
                  .headlineSmall
                  ?.copyWith(fontWeight: FontWeight.w700)),
          const SizedBox(height: 14),
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // 左：完整表单常驻（未选文件时各控件禁用）
                SizedBox(
                  width: 430,
                  child: _FormColumn(
                    active: active,
                    onPickSource: _pickSource,
                  ),
                ),
                const SizedBox(width: 16),
                // 右：预览（未选文件显示「暂无」）
                Expanded(child: _PreviewPane(draft: active)),
              ],
            ),
          ),
          const Divider(),
          Row(
            children: [
              Icon(Icons.info_outline_rounded, size: 15, color: cs.outline),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  'GDAL 绝对级别 · PNG 输出 · 输出目录自动创建层级与 preview.html',
                  style: TextStyle(fontSize: 12, color: cs.outline),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(width: 12),
              FilledButton.icon(
                onPressed: active == null ? null : () => _startAll(),
                icon: const Icon(Icons.play_arrow_rounded),
                label: const Text('开始切片'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _startAll() async {
    final store = ref.read(draftProvider);
    final app = ref.read(appProvider);
    // 双层阈值：>800万 弹确认（耗时/磁盘提示）；>1亿 硬拒绝（与 Rust 一致）
    const softLimit = 8000000;
    const hardLimit = 100000000;
    var pendingConfirm = false;
    for (final d in List<TaskDraft>.from(store.drafts)) {
      final total = d.estimates?.fold<int>(0, (a, e) => a + e.tiles.toInt()) ?? 0;
      if (total > hardLimit) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(SnackBar(
              content: Text(
                  '${d.fileName}：预计 $total 块瓦片，超过硬上限 $hardLimit（磁盘/耗时不可行）。请缩小级别范围。'),
              duration: const Duration(seconds: 4)));
        }
        return;
      }
      if (total > softLimit) pendingConfirm = true;
    }
    if (pendingConfirm && mounted) {
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text('确认大批量切片？'),
          content: const Text(
              '所选任务的预计瓦片总量超过 800 万块：\n'
              '· 耗时可能长达数小时至数天\n'
              '· 输出可能占用数百 GB 磁盘\n\n确定要继续吗？'),
          actions: [
            TextButton(
                onPressed: () => Navigator.pop(ctx, false),
                child: const Text('取消')),
            FilledButton(
                onPressed: () => Navigator.pop(ctx, true),
                child: const Text('继续切片')),
          ],
        ),
      );
      if (ok != true) return;
    }
    final failures = <String>[];
    // 预览底图配置：仅注入「设置中已启用」的条目（全部关闭 → 空数组，页面零在线依赖）
    final overlaysJson = jsonEncode(app.baseMaps
        .where((m) => m['on'] == true)
        .map((m) {
          final e = Map<String, dynamic>.from(m);
          // 注入该模板所需的所有密钥占位符（如 {tk} → mapKeys['tk']）
          for (final mm
              in RegExp(r'\{([a-zA-Z0-9]+)\}').allMatches(e['tpl'] as String)) {
            final name = mm.group(1)!;
            if (!{'z', 'x', 'y', 's'}.contains(name)) {
              e[name] = app.mapKeys[name] ?? '';
            }
          }
          return e;
        })
        .toList());
    for (final d in List<TaskDraft>.from(store.drafts)) {
      try {
        // 全局设置同步（瓦片尺寸/跳过透明/重采样在「设置」页维护）
        d.tileSize = app.tileSize;
        d.skipEmpty = app.skipEmpty;
        d.resample = app.resample;
        final id = await store.startDraft(d,
            outputOverride:
                app.defaultOutput.isNotEmpty ? d.outputDir : null,
            previewOverlays: overlaysJson);
        // 本地立即登记，任务中心即时可见（后续事件按此 id 更新）
        app.addLocalTask(id, d.toConfig(d.outputDir));
      } catch (e) {
        failures.add('${d.fileName}: $e');
      }
    }
    if (failures.isEmpty) {
      store.drafts.clear();
      store.activeIndex = 0;
      store.touch();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
            content: Text('已加入队列，可在「任务中心」查看进度')));
      }
    } else if (mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(failures.join('\n'))));
    }
  }
}

// ---------------- 左侧表单（常驻，未选文件时禁用） ----------------

TaskDraft _dummyDraft() => TaskDraft(
      source: '',
      fileName: '',
      width: 0,
      height: 0,
      maxLevel: 19,
    );

class _FormColumn extends ConsumerWidget {
  final TaskDraft? active;
  final Future<void> Function() onPickSource;
  const _FormColumn({required this.active, required this.onPickSource});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final store = ref.read(draftProvider);
    final d = active ?? _dummyDraft();
    final locked = active == null;
    final cs = Theme.of(context).colorScheme;

    return ListView(
      shrinkWrap: true,
      children: [
        // ---- 输入 ----
        _SectionCard(title: '输入', icon: Icons.file_open_rounded, children: [
          TextFormField(
            key: ValueKey(active?.source),
            initialValue: locked ? '' : active!.source,
            readOnly: true,
            style: const TextStyle(fontSize: 12.5),
            decoration: InputDecoration(
              labelText: '输入 TIFF 文件',
              hintText: locked ? '尚未选择文件' : null,
              prefixIcon: const Icon(Icons.image_rounded),
              suffixIcon: IconButton(
                tooltip: '选择 TIFF 文件',
                icon: const Icon(Icons.file_open_rounded),
                onPressed: onPickSource,
              ),
            ),
          ),
          if (!locked) ...[
            const SizedBox(height: 6),
            Row(children: [
              Icon(Icons.photo_size_select_large_rounded,
                  size: 14, color: cs.outline),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                    '${active!.width} × ${active!.height} px'
                    '${active!.nativeZoom != null ? ' · 原始≈Z${active!.nativeZoom}' : ''}',
                    style: TextStyle(fontSize: 11.5, color: cs.outline)),
              ),
            ]),
          ],
        ]),
        const SizedBox(height: 10),

        // ---- 输出 ----
        _SectionCard(title: '输出', icon: Icons.output_rounded, children: [
          TextFormField(
            key: ValueKey(active?.outputDir),
            initialValue: locked ? '' : active!.outputDir,
            enabled: !locked,
            decoration: InputDecoration(
              labelText: '输出目录',
              prefixIcon: const Icon(Icons.folder_rounded),
              suffixIcon: IconButton(
                tooltip: '选择文件夹',
                icon: const Icon(Icons.folder_open_rounded),
                onPressed: locked
                    ? null
                    : () async {
                        final picked = await FilePicker.getDirectoryPath(
                            dialogTitle: '选择输出目录');
                        if (picked != null && picked.isNotEmpty) {
                          active!.outputDir = picked;
                          store.touch();
                        }
                      },
              ),
            ),
            onChanged: (v) {
              if (!locked) active!.outputDir = v;
            },
          ),
          if (!locked && active!.estimateError.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(active!.estimateError,
                  style: TextStyle(fontSize: 11.5, color: cs.error)),
            ),
        ]),

        // ---- 级别范围 ----
        _SectionCard(title: '级别范围', icon: Icons.layers_rounded, children: [
          Builder(builder: (_) {
            // 防御性钳制：保证 min ≤ start ≤ end ≤ max，避免 RangeSlider 断言崩溃
            final sVal = d.zmin.clamp(1, 22).toDouble();
            final eVal =
                d.zmax < sVal ? sVal : d.zmax.clamp(sVal.toInt(), 22).toDouble();
            return RangeSlider(
              values: RangeValues(sVal, eVal),
              min: 1,
              max: 22,
              divisions: 21,
              labels: RangeLabels('Z${sVal.round()}', 'Z${eVal.round()}'),
              onChanged: locked
                  ? null
                  : (v) {
                      active!.zmin = v.start.round().clamp(1, 22);
                      active!.zmax = v.end.round().clamp(active!.zmin, 22);
                      store.refreshEstimates(active!);
                    },
            );
          }),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text('从 Z1 开始（不生成 Z0）',
                  style: TextStyle(fontSize: 11, color: Colors.grey.shade500)),
              Text(
                locked
                    ? 'GDAL 绝对 · Web-Mercator'
                    : '原始≈Z${active!.nativeZoom ?? '?'}'
                        '${active!.zmin > 0 ? ' · 跳过低级' : ''}',
                style: TextStyle(fontSize: 11, color: Colors.grey.shade500),
              ),
            ],
          ),
        ]),

        // ---- 排列方式 ----
        _SectionCard(title: '排列方式', icon: Icons.grid_view_rounded, children: [
          IgnorePointer(
            ignoring: locked,
            child: SegmentedButton<Scheme>(
              segments: const [
                ButtonSegment(value: Scheme.xyz, label: Text('XYZ')),
                ButtonSegment(value: Scheme.tms, label: Text('TMS')),
              ],
              selected: {d.scheme},
              onSelectionChanged: (s) {
                if (locked) return;
                active!.scheme = s.first;
                store.touch();
              },
            ),
          ),
          const SizedBox(height: 4),
          Text(
            d.scheme == Scheme.xyz
                ? '{输出}/{z}/{x}/{y}.png — Google/OSM 兼容'
                : '{输出}/{z}/{x}/{y}.png — Y 轴向下翻转',
            style: TextStyle(fontSize: 11, color: Colors.grey.shade500),
          ),
        ]),

        // ---- 透明处理 ----
        _SectionCard(title: '透明处理', icon: Icons.opacity_rounded, children: [
          DropdownButtonFormField<_AlphaChoice>(
            key: ValueKey(active?.alpha),
            initialValue: _AlphaChoiceX.of(d.alpha),
            items: const [
              DropdownMenuItem(value: _AlphaChoice.keep, child: Text('保留源透明通道')),
              DropdownMenuItem(value: _AlphaChoice.threshold, child: Text('Alpha 阈值 → 全透明')),
              DropdownMenuItem(value: _AlphaChoice.colorKey, child: Text('颜色键 → 透明')),
            ],
            onChanged: locked
                ? null
                : (v) {
                    switch (v!) {
                      case _AlphaChoice.keep:
                        active!.alpha = const AlphaMode.keep();
                      case _AlphaChoice.threshold:
                        active!.alpha = const AlphaMode.threshold(below: 128);
                      case _AlphaChoice.colorKey:
                        active!.alpha =
                            AlphaMode.colorKey(r: 255, g: 255, b: 255, tolerance: 12);
                    }
                    store.touch();
                  },
          ),
          if (!locked)
            ...switch (active!.alpha) {
              AlphaMode_Keep() => [const SizedBox(height: 4)],
              AlphaMode_Threshold(:final below) => [
                  Slider(
                    value: below.toDouble(),
                    min: 1,
                    max: 254,
                    label: '$below',
                    divisions: 253,
                    onChanged: (v) {
                      active!.alpha = AlphaMode.threshold(below: v.round());
                      store.touch();
                    },
                  ),
                  Text('低于 $below 的像素将被置为完全透明',
                      style:
                          TextStyle(fontSize: 11, color: Colors.grey.shade500)),
                ],
              AlphaMode_ColorKey(:final r, :final g, :final b, :final tolerance) => [
                  const SizedBox(height: 6),
                  Row(
                    children: [
                      Container(width: 16, height: 16,
                        decoration: BoxDecoration(
                          color: Color.fromARGB(255, r, g, b),
                          shape: BoxShape.circle,
                          border: Border.all(color: Colors.white24),
                        ),
                      ),
                      const SizedBox(width: 6),
                      _NumField(label: 'R', value: r, onChanged: (v) {
                        final a = active!.alpha as AlphaMode_ColorKey;
                        active!.alpha = AlphaMode.colorKey(
                            r: v, g: a.g, b: a.b, tolerance: a.tolerance);
                        store.touch();
                      }),
                      const SizedBox(width: 6),
                      _NumField(label: 'G', value: g, onChanged: (v) {
                        final a = active!.alpha as AlphaMode_ColorKey;
                        active!.alpha = AlphaMode.colorKey(
                            r: a.r, g: v, b: a.b, tolerance: a.tolerance);
                        store.touch();
                      }),
                      const SizedBox(width: 6),
                      _NumField(label: 'B', value: b, onChanged: (v) {
                        final a = active!.alpha as AlphaMode_ColorKey;
                        active!.alpha = AlphaMode.colorKey(
                            r: a.r, g: a.g, b: v, tolerance: a.tolerance);
                        store.touch();
                      }),
                      const SizedBox(width: 6),
                      _NumField(label: '容差', value: tolerance, onChanged: (v) {
                        final a = active!.alpha as AlphaMode_ColorKey;
                        active!.alpha = AlphaMode.colorKey(
                            r: a.r, g: a.g, b: a.b, tolerance: v);
                        store.touch();
                      }),
                    ],
                  ),
                  Text(
                      '可直接输入 RGB 与容差；也可从预览图像直接取色（下方按钮）',
                      style:
                          TextStyle(fontSize: 11, color: Colors.grey.shade500)),
                  const SizedBox(height: 8),
                  // 拾色按钮：仅颜色键模式下显示；位于左侧表单，不覆盖右侧预览图
                  SizedBox(
                    width: double.infinity,
                    child: FilledButton.tonalIcon(
                      onPressed: locked
                          ? null
                          : () {
                              active!.pickColorMode = !active!.pickColorMode;
                              store.touch();
                            },
                      icon: const Icon(Icons.colorize_rounded, size: 18),
                      label: Text(
                        active!.pickColorMode
                            ? '拾色中：点击右侧预览图选色（再点退出）'
                            : '从预览图像取色',
                        style: const TextStyle(fontSize: 12.5),
                      ),
                    ),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    active!.pickColorMode
                        ? '拾色模式已开启：点击右侧预览图上的目标颜色，将自动设为透明键'
                        : '点击后进入拾色模式，再点击右侧预览图上的颜色即可设为透明键',
                    style: TextStyle(fontSize: 11, color: Colors.grey.shade500),
                  ),
                ],
            },
        ]),
      ],
    );
  }
}
enum _AlphaChoice { keep, threshold, colorKey }

extension _AlphaChoiceX on _AlphaChoice {
  static _AlphaChoice of(AlphaMode m) => switch (m) {
        AlphaMode_Keep() => _AlphaChoice.keep,
        AlphaMode_Threshold() => _AlphaChoice.threshold,
        AlphaMode_ColorKey() => _AlphaChoice.colorKey,
      };
}

class _NumField extends StatelessWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;
  const _NumField({required this.label, required this.value, required this.onChanged});

  @override
  Widget build(BuildContext context) {
    final ctrl = TextEditingController(text: '$value');
    return Expanded(
      child: TextField(
        controller: ctrl,
        keyboardType: TextInputType.number,
        decoration: InputDecoration(labelText: label, isDense: true),
        onSubmitted: (v) {
          final n = int.tryParse(v);
          if (n != null && n >= 0 && n <= 255) onChanged(n);
        },
      ),
    );
  }
}

class _SectionCard extends StatelessWidget {
  final String title;
  final IconData icon;
  final List<Widget> children;
  const _SectionCard({required this.title, required this.icon, required this.children});

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Card(
      margin: const EdgeInsets.only(bottom: 10),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(children: [
              Icon(icon, size: 15, color: cs.primary),
              const SizedBox(width: 6),
              Text(title,
                  style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 13)),
            ]),
            const SizedBox(height: 10),
            ...children,
          ],
        ),
      ),
    );
  }
}

// ---------------- 右侧预览 ----------------

class _PreviewPane extends ConsumerStatefulWidget {
  final TaskDraft? draft;
  const _PreviewPane({required this.draft});

  @override
  ConsumerState<_PreviewPane> createState() => _PreviewPaneState();
}

class _PreviewPaneState extends ConsumerState<_PreviewPane> {
  Size? _previewSize;          // 预览 PNG 实际像素尺寸
  Uint8List? _decodedFor;      // 已解码的预览字节（同一 TaskDraft 内可变，需身份比较）
  Offset? _pickMark;           // 取色标记（预览像素坐标）
  Color? _pickColor;
  Timer? _markTimer;
  Timer? _refreshDebounce;     // 透明模式变化 → 防抖刷新预览
  AlphaMode? _lastAlpha;       // 上一次看到的透明模式（草稿原地修改，需自行记录）

  void _schedulePreviewRefresh(TaskDraft d) {
    _refreshDebounce?.cancel();
    _refreshDebounce = Timer(const Duration(milliseconds: 350), () {
      if (mounted && identical(widget.draft, d)) {
        ref.read(draftProvider).refreshPreview(d);
      }
    });
  }

  /// 从 PNG 字节直接解析 IHDR 尺寸（字节 16..24，大端宽高），
  /// 避免用与实际分辨率不符的占位公式导致布局错乱/拉伸花屏。
  Size _pngSize(Uint8List b) {
    if (b.length >= 24) {
      final rd = ByteData.sublistView(b, 16, 24);
      final w = rd.getUint32(0, Endian.big);
      final h = rd.getUint32(4, Endian.big);
      if (w > 0 && h > 0) return Size(w.toDouble(), h.toDouble());
    }
    return const Size(1400, 1100);
  }

  Size _expectedSize(TaskDraft d) =>
      d.previewBytes != null ? _pngSize(d.previewBytes!) : const Size(320, 260);

  void _scheduleDecode() {
    final bytes = widget.draft?.previewBytes;
    if (bytes == null || identical(_decodedFor, bytes)) return;
    _decodedFor = bytes;
    decodeImageFromList(bytes).then((img) {
      if (!mounted || !identical(_decodedFor, bytes)) return;
      setState(() => _previewSize =
          Size(img.width.toDouble(), img.height.toDouble()));
    });
  }

  @override
  void initState() {
    super.initState();
    _lastAlpha = widget.draft?.alpha;
    WidgetsBinding.instance.addPostFrameCallback((_) => _scheduleDecode());
  }

  @override
  void dispose() {
    _markTimer?.cancel();
    _refreshDebounce?.cancel();
    super.dispose();
  }

  @override
  void didUpdateWidget(covariant _PreviewPane old) {
    super.didUpdateWidget(old);
    if (!identical(old.draft?.previewBytes, widget.draft?.previewBytes)) {
      _previewSize = null;
      _pickMark = null;
    }
    // 透明模式变化 → 防抖刷新预览，立即看到新透明效果。
    // 注意：草稿对象原地修改，old/widget 的 draft 是同一对象，必须用本 State
    // 记录的上一次 alpha 比较，否则新旧值恒相等、刷新永不触发。
    final alpha = widget.draft?.alpha;
    if (alpha != _lastAlpha) {
      final hadPrev = _lastAlpha != null;
      _lastAlpha = alpha;
      final d = widget.draft;
      if (hadPrev && alpha != null && d != null) {
        _schedulePreviewRefresh(d);
      }
    }
    WidgetsBinding.instance.addPostFrameCallback((_) => _scheduleDecode());
  }

  /// 点击取色：仅颜色键模式 + 拾色模式下有效；
  /// 映射预览像素坐标 → 源坐标 → Rust 精确采样 → 写入颜色键。
  Future<void> _onPick(Offset localPx) async {
    final size = _previewSize;
    final draft = widget.draft;
    if (draft == null || draft.alpha is! AlphaMode_ColorKey) return;
    if (size == null || size.width < 1 || size.height < 1) return;
    final sx = (localPx.dx / size.width * draft.width)
        .floor()
        .clamp(0, draft.width - 1);
    final sy = (localPx.dy / size.height * draft.height)
        .floor()
        .clamp(0, draft.height - 1);
    try {
      final px = await rust.samplePixel(source: draft.source, x: sx, y: sy);
      final r = px[0], g = px[1], b = px[2];
      final a = draft.alpha as AlphaMode_ColorKey;
      final keepTol = a.tolerance;

      final prevKey = Color.fromARGB(255, a.r, a.g, a.b);
      final newKey = Color.fromARGB(255, r, g, b);

      draft.alpha = AlphaMode.colorKey(r: r, g: g, b: b, tolerance: keepTol);
      ref.read(draftProvider).touch();

      // 视觉反馈：图像内取色标记，900ms 淡出
      setState(() {
        _pickMark = localPx;
        _pickColor = newKey;
      });
      _markTimer?.cancel();
      _markTimer = Timer(const Duration(milliseconds: 900), () {
        if (mounted) setState(() => _pickMark = null);
      });

      // 打扰最小化：仅颜色实际变化时提示
      if (!mounted) return;
      if (prevKey != newKey) {
        ScaffoldMessenger.of(context).hideCurrentSnackBar();
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(
          content: Row(children: [
            Container(
                width: 13,
                height: 13,
                decoration: BoxDecoration(
                    color: newKey,
                    shape: BoxShape.circle,
                    border: Border.all(color: Colors.white24))),
            const SizedBox(width: 8),
            Text('透明键更新为 #${_hex(r, g, b)}'),
          ]),
          duration: const Duration(milliseconds: 1400),
        ));
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('取色失败: $e')));
      }
    }
  }

  static String _hex(int r, int g, int b) => [r, g, b]
      .map((v) => v.toRadixString(16).padLeft(2, '0'))
      .join()
      .toUpperCase();

  @override
  Widget build(BuildContext context) {
    // 监听草稿变化以刷新预览
    ref.watch(draftProvider);
    final cs = Theme.of(context).colorScheme;
    final draft = widget.draft;

    if (draft == null) {
      return Card(
        child: ClipRRect(
          borderRadius: BorderRadius.circular(16),
          child: Center(
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              Icon(Icons.image_not_supported_rounded,
                  size: 56, color: cs.outline),
              const SizedBox(height: 10),
              Text('暂无预览', style: TextStyle(color: cs.outline)),
            ]),
          ),
        ),
      );
    }

    final totalTiles = draft.estimates?.fold<int>(0, (a, e) => a + e.tiles.toInt()) ?? 0;

    return Card(
      child: ClipRRect(
        borderRadius: BorderRadius.circular(16),
        child: Column(
          children: [
            Expanded(
              child: Stack(
                fit: StackFit.expand,
                children: [
                  // 棋盘格底（透明度指示）：白色/浅色图像边缘、透明区域都能与预览底色区分
                  if (draft.previewBytes != null)
                    Positioned.fill(
                      child: CustomPaint(
                        painter: _CheckerboardPainter(
                            dark: Theme.of(context).brightness ==
                                Brightness.dark),
                      ),
                    ),
                  if (draft.previewBytes != null)
                    MouseRegion(
                      cursor: draft.pickColorMode &&
                              draft.alpha is AlphaMode_ColorKey
                          ? SystemMouseCursors.precise
                          : MouseCursor.defer,
                      child: InteractiveViewer(
                        maxScale: 12,
                        child: FittedBox(
                          fit: BoxFit.contain,
                          // 细边框勾勒图像范围（白边图上也能看清边界）
                          child: Container(
                            decoration: BoxDecoration(
                              border: Border.all(
                                  color: cs.outlineVariant
                                      .withValues(alpha: 0.9),
                                  width: 1),
                            ),
                            child: SizedBox(
                              width:
                                  (_previewSize ?? _expectedSize(draft)).width,
                              height:
                                  (_previewSize ?? _expectedSize(draft)).height,
                            child: Stack(
                              children: [
                                GestureDetector(
                                  behavior: HitTestBehavior.opaque,
                                  onTapUp:
                                      draft.pickColorMode &&
                                              draft.alpha is AlphaMode_ColorKey
                                          ? (d) => _onPick(d.localPosition)
                                          : null,
                                  child: Image.memory(draft.previewBytes!,
                                      fit: BoxFit.fill, gaplessPlayback: true),
                                ),
                                // 取色标记：点击位置短暂高亮
                                if (_pickMark != null && _pickColor != null)
                                  Positioned(
                                    left: _pickMark!.dx - 11,
                                    top: _pickMark!.dy - 11,
                                    child: IgnorePointer(
                                      child: TweenAnimationBuilder<double>(
                                        tween: Tween(begin: 0.4, end: 1),
                                        duration:
                                            const Duration(milliseconds: 120),
                                        builder: (context, v, _) => Container(
                                          width: 22,
                                          height: 22,
                                          decoration: BoxDecoration(
                                            shape: BoxShape.circle,
                                            border: Border.all(
                                                color: Colors.white
                                                    .withValues(alpha: v),
                                                width: 2),
                                            color: Colors.black26,
                                          ),
                                          alignment: Alignment.center,
                                          child: Container(
                                            width: 10,
                                            height: 10,
                                            decoration: BoxDecoration(
                                              shape: BoxShape.circle,
                                              color: _pickColor!
                                                  .withValues(alpha: v),
                                            ),
                                          ),
                                        ),
                                      ),
                                    ),
                                  ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                  // 仅在无图时显示状态：加载中转圈、失败提示、其余“暂无预览”。
                  // 成功出图后不再叠加占位文字——此前无条件显示“预览生成失败”会盖住正常预览。
                  if (draft.previewBytes == null)
                    Center(
                      child: draft.loadingPreview
                          ? const CircularProgressIndicator()
                          : Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(Icons.hide_image_rounded,
                                    size: 52, color: cs.outline),
                                const SizedBox(height: 8),
                                Text(
                                  draft.previewError.isNotEmpty
                                      ? '预览生成失败'
                                      : '暂无预览',
                                  style: TextStyle(color: cs.outline),
                                ),
                                if (draft.previewError.isNotEmpty)
                                  Padding(
                                    padding: const EdgeInsets.symmetric(
                                        horizontal: 24, vertical: 6),
                                    child: Text(
                                      draft.previewError,
                                      textAlign: TextAlign.center,
                                      maxLines: 3,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(
                                          fontSize: 11, color: cs.outline),
                                    ),
                                  ),
                              ],
                            ),
                    ),
                  if (draft.loadingPreview && draft.previewBytes == null)
                    Container(color: cs.scrim.withValues(alpha: 0.05)),
                  // 右上角：层级/排列信息角标
                  if (draft.previewBytes != null)
                    Positioned(
                      right: 10, top: 10,
                      child: IgnorePointer(
                        child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
                          decoration: BoxDecoration(
                            color: cs.scrim.withValues(alpha: 0.55),
                            borderRadius: BorderRadius.circular(999),
                          ),
                          child: Row(mainAxisSize: MainAxisSize.min, children: [
                            Icon(Icons.layers_rounded, size: 12, color: cs.primary),
                            const SizedBox(width: 5),
                            Text('Z${draft.zmin}–Z${draft.zmax} · ${schemeName(draft.scheme)}',
                                style: TextStyle(
                                    fontSize: 11, fontWeight: FontWeight.w600,
                                    color: cs.onSurface.withValues(alpha: 0.9))),
                          ]),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            Container(
              padding: const EdgeInsets.only(left: 14, right: 8, top: 6, bottom: 6),
              decoration: BoxDecoration(
                color: cs.surfaceContainerHighest.withValues(alpha: 0.35),
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Wrap(
                      spacing: 14,
                      runSpacing: 4,
                      children: [
                        _info(context, Icons.aspect_ratio_rounded,
                            '${draft.width} × ${draft.height} px'),
                        _info(context, Icons.hd_rounded,
                            '${fmtBytes(draft.width * draft.height * 4)} RGBA'),
                        if (totalTiles > 0)
                          _info(context, Icons.grid_on_rounded,
                              '预计 $totalTiles 块瓦片 · ${draft.estimates!.length} 个级别'),
                      ],
                    ),
                  ),
                  // 手动重新生成预览（拾取/改键后立即按当前透明模式重绘；位于图像下方不遮挡）
                  TextButton.icon(
                    onPressed: draft.previewBytes == null
                        ? null
                        : () => ref.read(draftProvider).refreshPreview(draft),
                    icon: const Icon(Icons.refresh_rounded, size: 16),
                    label: const Text('重新生成预览',
                        style: TextStyle(fontSize: 12)),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      padding: const EdgeInsets.symmetric(horizontal: 10),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _info(BuildContext context, IconData icon, String text) {
    final cs = Theme.of(context).colorScheme;
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Icon(icon, size: 14, color: cs.primary),
      const SizedBox(width: 5),
      Text(text, style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600)),
    ]);
  }
}

/// 棋盘格底（透明度指示）：白色/浅色图像边缘与透明区域都能和预览底色区分。
class _CheckerboardPainter extends CustomPainter {
  final bool dark;
  const _CheckerboardPainter({required this.dark});

  @override
  void paint(Canvas canvas, Size size) {
    const cell = 10.0;
    final c1 = dark ? const Color(0xFF2B2B2B) : const Color(0xFFFFFFFF);
    final c2 = dark ? const Color(0xFF3F3F3F) : const Color(0xFFC9C9C9);
    canvas.drawRect(Offset.zero & size, Paint()..color = c1);
    final p2 = Paint()..color = c2;
    for (int y = 0; y * cell < size.height; y++) {
      for (int x = 0; x * cell < size.width; x++) {
        if ((x + y).isEven) continue;
        canvas.drawRect(Rect.fromLTWH(x * cell, y * cell, cell, cell), p2);
      }
    }
  }

  @override
  bool shouldRepaint(covariant _CheckerboardPainter oldDelegate) =>
      oldDelegate.dark != dark;
}
