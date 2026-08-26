import 'dart:async';
import 'dart:io';
import 'dart:typed_data' show Uint8List;

import 'package:desktop_drop/desktop_drop.dart';
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
  bool _dragging = false;

  Future<void> _pickFiles() async {
    final files = await FilePicker.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['tif', 'tiff'],
      // 多选仍需此参数（v12 标记弃用但为多选唯一途径）
      // ignore: deprecated_member_use
      allowMultiple: true,
    );
    for (final f in files) {
      final p = f.path;
      if (p != null && p.isNotEmpty) await _addSource(p);
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
      // 默认输出目录：<源目录>_tiles；或全局默认
      final dir = Directory(path).parent.path;
      draft.outputDir = app.defaultOutput.isNotEmpty ? app.defaultOutput : '${dir}_tiles';
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
            child: active == null
                ? _EmptyDropZone(
                    dragging: _dragging,
                    onDragEnter: () => setState(() => _dragging = true),
                    onDragExit: () => setState(() => _dragging = false),
                    onFilesDropped: (files) async {
                      setState(() => _dragging = false);
                      for (final f in files) {
                        final p = f.toLowerCase();
                        if (p.endsWith('.tif') || p.endsWith('.tiff')) {
                          await _addSource(f);
                        }
                      }
                    },
                    onPick: _pickFiles,
                  )
                : Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // 左：文件 chips + 表单
                      SizedBox(
                        width: 430,
                        child: _FormColumn(active: active),
                      ),
                      const SizedBox(width: 16),
                      // 右：预览
                      Expanded(child: _PreviewPane(draft: active)),
                    ],
                  ),
          ),
          if (active != null) ...[
            const Divider(),
            Row(
              children: [
                Icon(Icons.info_outline_rounded, size: 15, color: cs.outline),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    '输出格式 PNG · 输出目录将自动创建层级文件夹与 preview.html',
                    style: TextStyle(fontSize: 12, color: cs.outline),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 12),
                FilledButton.icon(
                  onPressed: () => _startAll(),
                  icon: const Icon(Icons.play_arrow_rounded),
                  label: Text(drafts.drafts.length > 1
                      ? '开始全部（${drafts.drafts.length}）'
                      : '开始切片'),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _startAll() async {
    final store = ref.read(draftProvider);
    final app = ref.read(appProvider);
    final failures = <String>[];
    for (final d in List<TaskDraft>.from(store.drafts)) {
      try {
        await store.startDraft(d, outputOverride:
            app.defaultOutput.isNotEmpty ? d.outputDir : null);
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

// ---------------- 空态拖拽区 ----------------

class _EmptyDropZone extends StatelessWidget {
  final bool dragging;
  final VoidCallback onDragEnter;
  final VoidCallback onDragExit;
  final ValueChanged<List<String>> onFilesDropped;
  final VoidCallback onPick;

  const _EmptyDropZone({
    required this.dragging,
    required this.onDragEnter,
    required this.onDragExit,
    required this.onFilesDropped,
    required this.onPick,
  });

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Card(
      child: DropTarget(
        onDragEntered: (_) => onDragEnter(),
        onDragExited: (_) => onDragExit(),
        onDragDone: (details) =>
            onFilesDropped(details.files.map((f) => f.path).toList()),
        child: Container(
          width: double.infinity,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(16),
            border: Border.all(
              color: dragging ? cs.primary : cs.outlineVariant.withValues(alpha: 0.5),
              width: dragging ? 2 : 1.4,
            ),
          ),
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                AnimatedScale(
                  scale: dragging ? 1.12 : 1,
                  duration: const Duration(milliseconds: 180),
                  child: Icon(Icons.upload_file_rounded,
                      size: 64,
                      color: dragging ? cs.primary : cs.outline),
                ),
                const SizedBox(height: 14),
                Text(dragging ? '松开即可添加' : '拖入 .tif / .tiff 文件',
                    style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 6),
                Text('支持同时选择多个文件并行切片',
                    style: TextStyle(color: cs.outline, fontSize: 12)),
                const SizedBox(height: 18),
                FilledButton.icon(
                  onPressed: onPick,
                  icon: const Icon(Icons.file_open_rounded),
                  label: const Text('选择 TIFF 文件'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// ---------------- 左侧表单 ----------------

class _FormColumn extends ConsumerWidget {
  final TaskDraft active;
  const _FormColumn({required this.active});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final store = ref.read(draftProvider);
    return ListView(
      shrinkWrap: true,
      children: [
        // 文件 chips
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (var i = 0; i < store.drafts.length; i++)
              InputChip(
                selected: i == store.activeIndex,
                label: Text(store.drafts[i].fileName,
                    maxLines: 1, overflow: TextOverflow.ellipsis),
                onPressed: () => store.select(i),
                onDeleted: () => store.removeAt(i),
                avatar: Icon(Icons.image_rounded,
                    size: 15, color: Theme.of(context).colorScheme.primary),
              ),
          ],
        ),
        const SizedBox(height: 10),

        _SectionCard(title: '输出', icon: Icons.output_rounded, children: [
          TextFormField(
            initialValue: active.outputDir,
            decoration: const InputDecoration(
                labelText: '输出目录', prefixIcon: Icon(Icons.folder_rounded)),
            onChanged: (v) => active.outputDir = v,
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              const Text('瓦片尺寸'),
              const Spacer(),
              SegmentedButton<int>(
                segments: const [
                  ButtonSegment(value: 256, label: Text('256')),
                  ButtonSegment(value: 512, label: Text('512')),
                ],
                selected: {active.tileSize},
                onSelectionChanged: (s) {
                  active.tileSize = s.first;
                  store.refreshEstimates(active);
                },
              ),
            ],
          ),
        ]),

        _SectionCard(title: '级别范围', icon: Icons.layers_rounded, children: [
          RangeSlider(
            values: RangeValues(active.zmin.toDouble(), active.zmax.toDouble()),
            min: 0,
            // 允许超出原始分辨率最多 +3 级（放大输出）
            max: (active.maxLevel + 3).toDouble(),
            divisions: active.maxLevel + 3,
            labels: RangeLabels('Z${active.zmin}', 'Z${active.zmax}'),
            onChanged: (v) {
              active.zmin = v.start.round();
              active.zmax = v.end.round();
              store.refreshEstimates(active);
            },
          ),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text('Z0（单瓦片全览）',
                  style: TextStyle(fontSize: 11, color: Colors.grey.shade500)),
              Text(
                'Z${active.maxLevel} = 原始分辨率'
                '${active.zmax > active.maxLevel ? ' · 已超采样至 Z${active.zmax}（放大 ${(1 << (active.zmax - active.maxLevel))}×）' : ' · 可再 +3 级'}',
                style: TextStyle(
                    fontSize: 11,
                    color: active.zmax > active.maxLevel
                        ? Theme.of(context).colorScheme.primary
                        : Colors.grey.shade500),
              ),
            ],
          ),
        ]),

        _SectionCard(title: '排列方式', icon: Icons.grid_view_rounded, children: [
          SegmentedButton<Scheme>(
            segments: const [
              ButtonSegment(value: Scheme.xyz, label: Text('XYZ')),
              ButtonSegment(value: Scheme.tms, label: Text('TMS')),
            ],
            selected: {active.scheme},
            onSelectionChanged: (s) {
              active.scheme = s.first;
              store.touch();
            },
          ),
          const SizedBox(height: 4),
          Text(
            active.scheme == Scheme.xyz
                ? '{输出}/{z}/{x}/{y}.png — Google/OSM 兼容'
                : '{输出}/{z}/{x}/{y}.png — Y 轴向下翻转',
            style: TextStyle(fontSize: 11, color: Colors.grey.shade500),
          ),
        ]),

        _SectionCard(title: '透明处理', icon: Icons.opacity_rounded, children: [
          DropdownButtonFormField<_AlphaChoice>(
            // 外部（如预览点选取色）改变 alpha 时强制重建以同步选中项
            key: ValueKey(active.alpha),
            initialValue: _AlphaChoiceX.of(active.alpha),
            items: const [
              DropdownMenuItem(value: _AlphaChoice.keep, child: Text('保留源透明通道')),
              DropdownMenuItem(value: _AlphaChoice.threshold, child: Text('Alpha 阈值 → 全透明')),
              DropdownMenuItem(value: _AlphaChoice.colorKey, child: Text('颜色键 → 透明（白底常用）')),
            ],
            onChanged: (v) {
              switch (v!) {
                case _AlphaChoice.keep:
                  active.alpha = const AlphaMode.keep();
                case _AlphaChoice.threshold:
                  active.alpha = const AlphaMode.threshold(below: 128);
                case _AlphaChoice.colorKey:
                  active.alpha = const AlphaMode.colorKey(r: 255, g: 255, b: 255, tolerance: 12);
              }
              store.touch();
            },
          ),
          ...switch (active.alpha) {
            AlphaMode_Keep() => [const SizedBox(height: 4)],
            AlphaMode_Threshold(:final below) => [
                Slider(
                  value: below.toDouble(),
                  min: 1,
                  max: 254,
                  label: '$below',
                  divisions: 253,
                  onChanged: (v) {
                    active.alpha = AlphaMode.threshold(below: v.round());
                    store.touch();
                  },
                ),
                Text('低于 $below 的像素将被置为完全透明',
                    style: TextStyle(fontSize: 11, color: Colors.grey.shade500)),
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
                      final a = active.alpha as AlphaMode_ColorKey;
                      active.alpha = AlphaMode.colorKey(r: v, g: a.g, b: a.b, tolerance: a.tolerance);
                      store.touch();
                    }),
                    const SizedBox(width: 6),
                    _NumField(label: 'G', value: g, onChanged: (v) {
                      final a = active.alpha as AlphaMode_ColorKey;
                      active.alpha = AlphaMode.colorKey(r: a.r, g: v, b: a.b, tolerance: a.tolerance);
                      store.touch();
                    }),
                    const SizedBox(width: 6),
                    _NumField(label: 'B', value: b, onChanged: (v) {
                      final a = active.alpha as AlphaMode_ColorKey;
                      active.alpha = AlphaMode.colorKey(r: a.r, g: a.g, b: v, tolerance: a.tolerance);
                      store.touch();
                    }),
                    const SizedBox(width: 6),
                    _NumField(label: '容差', value: tolerance, onChanged: (v) {
                      final a = active.alpha as AlphaMode_ColorKey;
                      active.alpha = AlphaMode.colorKey(r: a.r, g: a.g, b: a.b, tolerance: v);
                      store.touch();
                    }),
                  ],
                ),
                Text('可直接在右侧预览图上点击取色，容差 $tolerance',
                    style: TextStyle(fontSize: 11, color: Colors.grey.shade500)),
              ],
          },
        ]),

        _SectionCard(title: '重采样', icon: Icons.texture_rounded, children: [
          SegmentedButton<Resample>(
            segments: const [
              ButtonSegment(value: Resample.nearest, label: Text('最近邻')),
              ButtonSegment(value: Resample.bilinear, label: Text('双线性')),
            ],
            selected: {active.resample},
            onSelectionChanged: (s) {
              active.resample = s.first;
              store.touch();
            },
          ),
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
  final TaskDraft draft;
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

  /// 预览未解码前的占位尺寸（与 Rust makePreview 同公式），避免 1×1 不可见。
  Size _expectedSize(TaskDraft d) {
    final long = (d.width > d.height ? d.width : d.height).clamp(1, 1 << 30);
    final scale = (long / 2048).ceil().clamp(1, 1 << 20);
    return Size((d.width / scale).ceilToDouble(),
        (d.height / scale).ceilToDouble());
  }

  void _scheduleDecode() {
    final bytes = widget.draft.previewBytes;
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
    WidgetsBinding.instance.addPostFrameCallback((_) => _scheduleDecode());
  }

  @override
  void didUpdateWidget(covariant _PreviewPane old) {
    super.didUpdateWidget(old);
    if (!identical(old.draft.previewBytes, widget.draft.previewBytes)) {
      _previewSize = null;
      _pickMark = null;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) => _scheduleDecode());
  }

  @override
  void dispose() {
    _markTimer?.cancel();
    super.dispose();
  }

  /// 点击取色：仅颜色键模式下有效；映射源坐标 → Rust 精确采样 → 写入颜色键。
  Future<void> _onPick(Offset localPx) async {
    final size = _previewSize;
    final draft = widget.draft;
    if (size == null || size.width < 1 || size.height < 1) return;
    // 门控：只有颜色键模式才拾取，避免无意义改参
    if (draft.alpha is! AlphaMode_ColorKey) {
      ScaffoldMessenger.of(context).hideCurrentSnackBar();
      ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
        content: Text('请先在左侧「透明处理」中选择「颜色键 → 透明」，再点击图像取色'),
        duration: Duration(seconds: 2),
      ));
      return;
    }
    final sx = (localPx.dx / size.width * draft.width).floor().clamp(0, draft.width - 1);
    final sy = (localPx.dy / size.height * draft.height).floor().clamp(0, draft.height - 1);
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
            Container(width: 13, height: 13,
                decoration: BoxDecoration(color: newKey, shape: BoxShape.circle,
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

  static String _hex(int r, int g, int b) =>
      [r, g, b].map((v) => v.toRadixString(16).padLeft(2, '0')).join().toUpperCase();

  @override
  Widget build(BuildContext context) {
    // 监听草稿变化以刷新预览
    ref.watch(draftProvider);
    final cs = Theme.of(context).colorScheme;
    final draft = widget.draft;

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
                  if (draft.previewBytes != null)
                    MouseRegion(
                      cursor: draft.alpha is AlphaMode_ColorKey
                          ? SystemMouseCursors.precise
                          : MouseCursor.defer,
                      child: InteractiveViewer(
                        maxScale: 12,
                        child: FittedBox(
                          fit: BoxFit.contain,
                          child: SizedBox(
                            width: (_previewSize ?? _expectedSize(draft)).width,
                            height: (_previewSize ?? _expectedSize(draft)).height,
                            child: Stack(
                              children: [
                                GestureDetector(
                                  behavior: HitTestBehavior.opaque,
                                  onTapUp: (d) => _onPick(d.localPosition),
                                  child: Image.memory(draft.previewBytes!,
                                      fit: BoxFit.fill, gaplessPlayback: true),
                                ),
                                if (_pickMark != null && _pickColor != null)
                                  Positioned(
                                    left: _pickMark!.dx - 11,
                                    top: _pickMark!.dy - 11,
                                    child: IgnorePointer(
                                      child: TweenAnimationBuilder<double>(
                                        tween: Tween(begin: 0.4, end: 1),
                                        duration: const Duration(milliseconds: 120),
                                        builder: (context, v, _) => Container(
                                          width: 22, height: 22,
                                          decoration: BoxDecoration(
                                            shape: BoxShape.circle,
                                            border: Border.all(
                                                color: Colors.white.withValues(alpha: v),
                                                width: 2),
                                            color: Colors.black26,
                                          ),
                                          alignment: Alignment.center,
                                          child: Container(
                                            width: 10, height: 10,
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
                    )
                  else
                    Center(
                      child: draft.loadingPreview
                          ? const CircularProgressIndicator()
                          : Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(Icons.hide_image_rounded,
                                    size: 52, color: cs.outline),
                                const SizedBox(height: 8),
                                Text('预览生成失败',
                                    style: TextStyle(color: cs.outline)),
                              ],
                            ),
                    ),
                  if (draft.loadingPreview && draft.previewBytes == null)
                    Container(color: cs.scrim.withValues(alpha: 0.05)),
                  // 右上角：层级/排列信息 + 取色提示角标
                  if (draft.previewBytes != null)
                    Positioned(
                      right: 10, top: 10,
                      child: IgnorePointer(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.end,
                          children: [
                            Container(
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
                            const SizedBox(height: 5),
                            Container(
                              padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
                              decoration: BoxDecoration(
                                color: cs.scrim.withValues(alpha: 0.55),
                                borderRadius: BorderRadius.circular(999),
                              ),
                              child: Row(mainAxisSize: MainAxisSize.min, children: [
                                Icon(
                                  draft.alpha is AlphaMode_ColorKey
                                      ? Icons.colorize_rounded
                                      : Icons.lock_outline_rounded,
                                  size: 12,
                                  color: draft.alpha is AlphaMode_ColorKey
                                      ? cs.primary
                                      : cs.outline,
                                ),
                                const SizedBox(width: 5),
                                Text(
                                  draft.alpha is AlphaMode_ColorKey
                                      ? '点击图像拾取透明色'
                                      : '选「颜色键」后可点图取色',
                                  style: TextStyle(
                                      fontSize: 11,
                                      color: cs.onSurface
                                          .withValues(alpha: draft.alpha is AlphaMode_ColorKey ? 0.85 : 0.55)),
                                ),
                              ]),
                            ),
                          ],
                        ),
                      ),
                    ),
                ],
              ),
            ),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              decoration: BoxDecoration(
                color: cs.surfaceContainerHighest.withValues(alpha: 0.35),
              ),
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
