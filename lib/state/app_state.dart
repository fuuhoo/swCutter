import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import 'package:sw_cutter/src/rust/api/task_api.dart' as api;
import 'package:sw_cutter/src/rust/engine/alpha.dart';
import 'package:sw_cutter/src/rust/engine/planner.dart';

/// 全局任务与设置状态。
class AppState extends ChangeNotifier {
  /// 任务快照，按创建顺序。
  final List<api.TaskDto> tasks = [];

  /// 每任务速度（字节/秒），由秒级 tick 计算。
  final Map<int, int> speedBps = {};
  Map<int, _Prev> _prev = {};

  Timer? _ticker;
  StreamSubscription<api.TaskEvent>? _sub;

  // ---- 设置 ----
  int concurrency = 2;
  String defaultOutput = '';
  ThemeMode themeMode = ThemeMode.dark;
  /// 全局瓦片尺寸
  int tileSize = 256;
  /// 全局：跳过全透明瓦片（默认关，与 gdal2tiles 行为一致）
  bool skipEmpty = false;
  /// 全局重采样方式
  Resample resample = Resample.bilinear;
  /// 地图服务密钥表（键名 = 模板占位符，如 {tk} → 'tk'；预览底图用）
  Map<String, String> mapKeys = {'tk': ''};
  /// 预览底图/叠加层（写入每个 preview.html 的 CFG.overlays）
  List<Map<String, dynamic>> baseMaps = [
    {
      'name': 'OpenStreetMap',
      'tpl': 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
      'subs': '', 'tms': false, 'on': true, 'below': true,
      'opacity': 1.0, 'zmin': 2, 'zmax': 19,
    },
  ];

  bool settingsLoaded = false;

  Future<void> loadSettings() async {
    try {
      final f = File(await _settingsPath());
      if (await f.exists()) {
        final j = jsonDecode(await f.readAsString()) as Map<String, dynamic>;
        concurrency = (j['concurrency'] as num?)?.toInt() ?? 2;
        defaultOutput = (j['defaultOutput'] as String?) ?? '';
        themeMode = ThemeMode.values.asMap()[j['themeMode'] ?? 0] ?? ThemeMode.dark;
        tileSize = (j['tileSize'] as num?)?.toInt() ?? 256;
        skipEmpty = (j['skipEmpty'] as bool?) ?? false;
        resample = (j['resample'] as int?) == 0 ? Resample.nearest : Resample.bilinear;
        final mk = j['mapKeys'];
        if (mk is Map) {
          mapKeys = mk.map((k, v) => MapEntry(k.toString(), v.toString()));
        } else {
          // 迁移旧版 tiandituTk → mapKeys['tk']
          mapKeys = {'tk': (j['tiandituTk'] as String?) ?? ''};
        }
        final bm = j['baseMaps'];
        if (bm is List && bm.isNotEmpty) {
          baseMaps = bm.map((e) => Map<String, dynamic>.from(e as Map)).toList();
        }
      }
      settingsLoaded = true;
      notifyListeners();
    } catch (_) {
      settingsLoaded = true;
    }
  }

  Future<void> saveSettings() async {
    try {
      final f = File(await _settingsPath());
      await f.parent.create(recursive: true);
      await f.writeAsString(jsonEncode({
        'concurrency': concurrency,
        'defaultOutput': defaultOutput,
        'themeMode': themeMode.index,
        'tileSize': tileSize,
        'skipEmpty': skipEmpty,
        'resample': resample == Resample.nearest ? 0 : 1,
        'mapKeys': mapKeys,
        'baseMaps': baseMaps,
      }));
    } catch (_) {}
  }

  Future<String> _settingsPath() async {
    final dir = await getApplicationSupportDirectory();
    return '${dir.path}${Platform.pathSeparator}settings.json';
  }

  /// 启动时调用：拉取历史任务 + 订阅事件流 + 开启速度 tick。
  Future<void> bootstrap() async {
    await loadSettings();
    try {
      await api.setMaxConcurrency(n: concurrency);
    } catch (_) {}
    try {
      tasks
        ..clear()
        ..addAll(await api.listTasks());
    } catch (_) {}
    _sub = api.subscribeEvents().listen(_onEvent, onError: (Object _) {});
    _ticker = Timer.periodic(const Duration(seconds: 1), (_) => _tick());
    notifyListeners();
  }

  void _onEvent(api.TaskEvent ev) {
    final id = ev.taskId.toInt();
    final idx = tasks.indexWhere((t) => t.id.toInt() == id);
    if (idx == -1) {
      // 未知任务事件（如冷启动竞态）：整表刷新兜底
      refreshTasks();
      return;
    }
    final k = ev.kind;
    if (k is api.TaskEventKind_StatusChanged) {
      tasks[idx] = _copyWith(tasks[idx], status: k.status);
    } else if (k is api.TaskEventKind_Started) {
      var t = _copyWith(
        tasks[idx],
        status: 'running',
        totalTiles: k.totalTiles.toInt(),
        startedAtMs: BigInt.from(DateTime.now().millisecondsSinceEpoch),
      );
      tasks[idx] = t;
    } else if (k is api.TaskEventKind_LevelStart) {
      tasks[idx] = _copyWith(tasks[idx], level: k.level);
    } else if (k is api.TaskEventKind_Progress) {
      tasks[idx] = _copyWith(
        tasks[idx],
        level: k.level,
        tilesDone: k.tilesDone.toInt(),
        totalTiles: k.totalTiles.toInt(),
        bytesWritten: k.bytesWritten.toInt(),
        elapsedMs: BigInt.from(k.elapsedMs.toInt()),
      );
    } else if (k is api.TaskEventKind_Finished) {
      final s = k.summary;
      var t = _copyWith(
        tasks[idx],
        tilesDone: s.tilesDone.toInt(),
        totalTiles: s.totalTiles.toInt(),
        bytesWritten: s.bytesWritten.toInt(),
        elapsedMs: s.elapsedMs,
        error: s.error,
        finishedAtMs: BigInt.from(DateTime.now().millisecondsSinceEpoch),
      );
      t = _copyWith(
        t,
        status: s.cancelled ? 'cancelled' : (s.error != null ? 'error' : 'done'),
      );
      tasks[idx] = t;
      speedBps.remove(id);
    }
    notifyListeners();
  }

  api.TaskDto _copyWith(
    api.TaskDto t, {
    String? status,
    int? level,
    int? tilesDone,
    int? totalTiles,
    int? bytesWritten,
    BigInt? elapsedMs,
    BigInt? startedAtMs,
    BigInt? finishedAtMs,
    String? error,
  }) {
    return api.TaskDto(
      id: t.id,
      source: t.source,
      output: t.output,
      tileSize: t.tileSize,
      scheme: t.scheme,
      alpha: t.alpha,
      resample: t.resample,
      zmin: t.zmin,
      zmax: t.zmax,
      status: status ?? t.status,
      level: level ?? t.level,
      tilesDone: BigInt.from(tilesDone ?? t.tilesDone.toInt()),
      totalTiles: BigInt.from(totalTiles ?? t.totalTiles.toInt()),
      bytesWritten: BigInt.from(bytesWritten ?? t.bytesWritten.toInt()),
      elapsedMs: elapsedMs ?? t.elapsedMs,
      startedAtMs: startedAtMs ?? t.startedAtMs,
      finishedAtMs: finishedAtMs ?? t.finishedAtMs,
      error: error ?? t.error,
    );
  }

  /// 本地立即登记新任务（不等事件），保证任务中心实时可见。
  void addLocalTask(int id, api.TaskConfig cfg) {
    if (tasks.any((t) => t.id.toInt() == id)) return;
    tasks.insert(
      0,
      api.TaskDto(
        id: BigInt.from(id),
        source: cfg.source,
        output: cfg.output,
        tileSize: cfg.tileSize,
        scheme: cfg.scheme,
        alpha: cfg.alpha,
        resample: cfg.resample,
        zmin: cfg.zmin,
        zmax: cfg.zmax,
        status: 'queued',
        level: 0,
        tilesDone: BigInt.zero,
        totalTiles: BigInt.zero,
        bytesWritten: BigInt.zero,
        elapsedMs: BigInt.zero,
        startedAtMs: BigInt.zero,
        finishedAtMs: BigInt.zero,
        error: null,
      ),
    );
    notifyListeners();
  }

  /// 从 Rust 端全量刷新任务列表。
  Future<void> refreshTasks() async {
    try {
      final list = await api.listTasks();
      tasks
        ..clear()
        ..addAll(list);
      notifyListeners();
    } catch (_) {}
  }

  void _tick() {
    final now = DateTime.now();
    final prev2 = <int, _Prev>{};
    for (final t in tasks) {
      final id = t.id.toInt();
      prev2[id] = _Prev(t.bytesWritten.toInt(), now);
      final p = _prev[id];
      if (p == null) continue;
      final dt = now.difference(p.at).inMilliseconds;
      if (dt > 400 && t.status == 'running') {
        speedBps[id] =
            ((t.bytesWritten.toInt() - p.bytes) * 1000 / dt).round().clamp(0, 1 << 40);
      } else if (t.status != 'running') {
        speedBps.remove(id);
      }
    }
    _prev = prev2;
    notifyListeners();
  }

  Future<void> setConcurrency(int n) async {
    concurrency = n.clamp(1, 16);
    try {
      await api.setMaxConcurrency(n: concurrency);
    } catch (_) {}
    await saveSettings();
    notifyListeners();
  }

  void setThemeMode(ThemeMode m) {
    themeMode = m;
    saveSettings();
    notifyListeners();
  }

  void setDefaultOutput(String p) {
    defaultOutput = p;
    saveSettings();
    notifyListeners();
  }

  void setTileSize(int v) {
    tileSize = v;
    saveSettings();
    notifyListeners();
  }

  void setSkipEmpty(bool v) {
    skipEmpty = v;
    saveSettings();
    notifyListeners();
  }

  void setResample(Resample v) {
    resample = v;
    saveSettings();
    notifyListeners();
  }

  /// 设置某个地图服务密钥（键名 = 模板占位符，如 'tk'）：输入即保存并刷新。
  void setMapKey(String name, String value) {
    mapKeys[name] = value;
    saveSettings();
    notifyListeners();
  }

  void addBasemap(Map<String, dynamic> layer) {
    baseMaps.add(layer);
    saveSettings();
    notifyListeners();
  }

  void removeBasemap(int i) {
    if (i < 0 || i >= baseMaps.length) return;
    baseMaps.removeAt(i);
    saveSettings();
    notifyListeners();
  }

  void setBasemapOn(int i, bool v) {
    if (i < 0 || i >= baseMaps.length) return;
    baseMaps[i]['on'] = v;
    saveSettings();
    notifyListeners();
  }

  void setBasemapBelow(int i, bool v) {
    if (i < 0 || i >= baseMaps.length) return;
    baseMaps[i]['below'] = v;
    saveSettings();
    notifyListeners();
  }

  /// 外部完成状态修改后手动刷新。
  void notifySelf() => notifyListeners();

  @override
  void dispose() {
    _ticker?.cancel();
    _sub?.cancel();
    super.dispose();
  }
}

class _Prev {
  final int bytes;
  final DateTime at;
  _Prev(this.bytes, this.at);
}

final appProvider = ChangeNotifierProvider<AppState>((ref) => AppState());

// ---------------- 任务草稿（新建任务页） ----------------

/// 一个待切片文件的草稿配置。
class TaskDraft {
  final String source;
  final String fileName;
  final int width;
  final int height;
  final int maxLevel;

  int tileSize = 256;
  Scheme scheme = Scheme.xyz;
  AlphaMode alpha = const AlphaMode.keep();
  Resample resample = Resample.bilinear;
  int zmin = 0;
  int zmax = 0;
  String outputDir = '';
  /// GDAL mercator 绝对级别模式（需 GeoTIFF 地理参考；zmax 截断 native）
  bool mercator = false;
  /// 全透明瓦片跳过写入（默认保留空白透明瓦片，对齐 gdal2tiles 行为）
  bool skipEmpty = false;
  Uint8List? previewBytes;
  String previewError = '';
  bool loadingPreview = false;
  /// 从预览图像取色模式（仅颜色键模式下可开启；表单按钮切换，预览图点击采样）
  bool pickColorMode = false;
  List<api.LevelEstimate>? estimates;
  /// 估算返回的 native zoom（mercator=GSD 级别；relative=原始级别）
  int? nativeZoom;
  String estimateError = '';

  TaskDraft({
    required this.source,
    required this.fileName,
    required this.width,
    required this.height,
    required this.maxLevel,
  }) {
    zmin = 0;
    zmax = maxLevel;
  }

  api.TaskConfig toConfig(String outputDir, {String? previewOverlays}) =>
      api.TaskConfig(
        source: source,
        output: outputDir,
        tileSize: tileSize,
        zmin: zmin,
        zmax: zmax,
        scheme: scheme,
        alpha: alpha,
        resample: resample,
        skipEmpty: skipEmpty,
        mercator: mercator,
        previewOverlays: previewOverlays,
      );
}

/// 新建任务页的草稿集合状态。
class DraftStore extends ChangeNotifier {
  final List<TaskDraft> drafts = [];
  int activeIndex = 0;
  String? lastError;

  TaskDraft? get active =>
      drafts.isEmpty || activeIndex >= drafts.length ? null : drafts[activeIndex];

  void add(TaskDraft d) {
    drafts.add(d);
    activeIndex = drafts.length - 1;
    notifyListeners();
  }

  void removeAt(int i) {
    if (i < 0 || i >= drafts.length) return;
    drafts.removeAt(i);
    if (activeIndex >= drafts.length) activeIndex = drafts.length - 1;
    notifyListeners();
  }

  void select(int i) {
    activeIndex = i;
    notifyListeners();
  }

  void touch() => notifyListeners();

  /// 透明模式变化后刷新预览（仅精图；解码块进程级缓存，二次调用接近瞬时）。
  /// 拾色/改键后即时看到新透明效果。
  Future<void> refreshPreview(TaskDraft d) async {
    try {
      final fine =
          await api.makePreview(source: d.source, maxPx: 1400, alpha: d.alpha);
      if (!identical(d, active) && !drafts.contains(d)) return;
      d.previewBytes = Uint8List.fromList(fine);
      notifyListeners();
    } catch (_) {
      // 刷新失败保留旧预览，不打扰
    }
  }

  /// 渐进式预览：先出低清（秒开），后台再补高清替换。
  Future<void> loadPreview(TaskDraft d, {int maxPx = 1400}) async {
    d.loadingPreview = true;
    d.previewError = '';
    notifyListeners();
    try {
      // 第一阶段：极低清快速出图（只触碰极少 chunk，10G+ 也 <2s）
      try {
        final coarse =
            await api.makePreview(source: d.source, maxPx: 320, alpha: d.alpha);
        // 防御性拷贝：脱离 FRB 缓冲，避免底层内存复用导致解码花屏
        final coarseCopy = Uint8List.fromList(coarse);
        // 用户可能已切换草稿或重入：仅当仍是当前字节时覆盖
        if (identical(d, active) || drafts.contains(d)) {
          d.previewBytes = coarseCopy;
          notifyListeners();
        }
      } catch (_) {/* 粗图失败不阻断高清 */}
      // 第二阶段：高清精修
      final fine = await api.makePreview(source: d.source, maxPx: maxPx, alpha: d.alpha);
      d.previewBytes = Uint8List.fromList(fine);
    } catch (e) {
      if (d.previewBytes == null) d.previewError = e.toString();
    } finally {
      d.loadingPreview = false;
      notifyListeners();
    }
  }

  Future<void> refreshEstimates(TaskDraft d) async {
    try {
      final est = await api.estimatePyramidEx(
        source: d.source,
        width: d.width,
        height: d.height,
        tileSize: d.tileSize,
        zmin: d.zmin,
        zmax: d.zmax,
        mercator: d.mercator,
      );
      d.estimates = est.levels;
      d.nativeZoom = est.nativeZoom?.toInt();
    } catch (e) {
      d.estimates = null;
      d.estimateError = e.toString();
    }
    notifyListeners();
  }

  Future<int> startDraft(TaskDraft d,
      {String? outputOverride, String? previewOverlays}) async {
    final out = (outputOverride ?? d.outputDir).trim();
    if (out.isEmpty) throw Exception('请先选择输出目录');
    d.outputDir = out;
    final id =
        await api.startTask(cfg: d.toConfig(out, previewOverlays: previewOverlays));
    return id.toInt();
  }
}

final draftProvider = ChangeNotifierProvider<DraftStore>((ref) => DraftStore());
