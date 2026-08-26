import 'dart:io';

import 'package:flutter/material.dart';

import '../src/rust/api/task_api.dart' as rust;
import '../src/rust/engine/alpha.dart';
import '../src/rust/engine/planner.dart';

String fmtBytes(int b) {
  if (b < 1024) return '$b B';
  if (b < 1024 * 1024) return '${(b / 1024).toStringAsFixed(1)} KB';
  if (b < 1024 * 1024 * 1024) return '${(b / 1024 / 1024).toStringAsFixed(1)} MB';
  return '${(b / 1024 / 1024 / 1024).toStringAsFixed(2)} GB';
}

String fmtDuration(int ms) {
  final d = Duration(milliseconds: ms);
  final h = d.inHours, m = d.inMinutes.remainder(60), s = d.inSeconds.remainder(60);
  if (h > 0) return '${h}h${m}m';
  if (m > 0) return '${m}m${s}s';
  return '${s}s';
}

/// Unix 毫秒 → 本地时间文本；今天只显示时分秒，往年月日。
String fmtTime(int ms) {
  if (ms <= 0) return '—';
  final dt = DateTime.fromMillisecondsSinceEpoch(ms);
  final now = DateTime.now();
  final sameDay = dt.year == now.year && dt.month == now.month && dt.day == now.day;
  String two(int v) => v.toString().padLeft(2, '0');
  final hms = '${two(dt.hour)}:${two(dt.minute)}:${two(dt.second)}';
  if (sameDay) return hms;
  return '${dt.year}-${two(dt.month)}-${two(dt.day)} $hms';
}

String schemeName(Scheme s) => s == Scheme.xyz ? 'XYZ' : 'TMS';

/// 状态徽章颜色。
Color statusColor(BuildContext context, String status) {
  final cs = Theme.of(context).colorScheme;
  switch (status) {
    case 'running':
      return cs.primary;
    case 'paused':
      return const Color(0xFFF5A524);
    case 'done':
      return const Color(0xFF34B37E);
    case 'error':
      return const Color(0xFFE5484D);
    case 'cancelled':
      return const Color(0xFFF5A524);
    default:
      return cs.outline;
  }
}

String statusLabel(String status) => switch (status) {
      'running' => '切片中',
      'paused' => '已暂停',
      'queued' => '排队中',
      'done' => '已完成',
      'error' => '失败',
      'cancelled' => '已取消',
      _ => status,
    };

String alphaLabel(AlphaMode a) => switch (a) {
      AlphaMode_Keep() => '保留透明',
      AlphaMode_Threshold(:final below) => '阈值 <$below',
      AlphaMode_ColorKey(:final r, :final g, :final b) =>
        '色键 #$r$g$b',
    };

class TaskCard extends StatelessWidget {
  final rust.TaskDto task;
  final int? speedBps;
  final VoidCallback onCancel;
  final VoidCallback onPauseResume;
  final VoidCallback onOpenFolder;
  final VoidCallback onPreview;
  final VoidCallback onRemove;

  const TaskCard({
    super.key,
    required this.task,
    required this.speedBps,
    required this.onCancel,
    required this.onPauseResume,
    required this.onOpenFolder,
    required this.onPreview,
    required this.onRemove,
  });

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final t = task;
    final running = t.status == 'running';
    final finished =
        t.status == 'done' || t.status == 'error' || t.status == 'cancelled';
    final progress = t.totalTiles == BigInt.zero
        ? (running ? null : 0.0)
        : (t.tilesDone.toInt() / t.totalTiles.toInt()).clamp(0.0, 1.0);

    // ETA：按平均单瓦片耗时估算
    String? eta;
    if (running && t.tilesDone != BigInt.zero && progress != null && progress > 0) {
      final remainTiles = t.totalTiles.toInt() - t.tilesDone.toInt();
      final msPerTile = t.elapsedMs.toInt() / t.tilesDone.toInt();
      if (msPerTile.isFinite && msPerTile > 0) {
        eta = fmtDuration((remainTiles * msPerTile).round());
      }
    }
    final speed = speedBps;

    final fileName = t.source.split(Platform.pathSeparator).last;

    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 14, 12, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(8),
                  decoration: BoxDecoration(
                    color: statusColor(context, t.status).withValues(alpha: 0.13),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Icon(_statusIcon(t.status),
                      size: 18, color: statusColor(context, t.status)),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    fileName,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 14),
                  ),
                ),
                _StatusChip(status: t.status),
              ],
            ),
            const SizedBox(height: 10),
            // 进度条 + 操作按钮同一行
            Row(
              children: [
                Expanded(
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(6),
                    child: TweenAnimationBuilder<double>(
                      tween: Tween(end: progress ?? 0),
                      duration: const Duration(milliseconds: 300),
                      curve: Curves.easeOutCubic,
                      builder: (context, v, _) => LinearProgressIndicator(
                        value: running ? null : v,
                        minHeight: 7,
                        backgroundColor:
                            cs.surfaceContainerHighest.withValues(alpha: 0.5),
                      ),
                    ),
                  ),
                ),
                if (progress != null) ...[
                  const SizedBox(width: 8),
                  Text('${(progress * 100).toStringAsFixed(0)}%',
                      style: TextStyle(
                          fontSize: 11.5,
                          fontFeatures: const [
                            FontFeature.tabularFigures()
                          ],
                          color: cs.onSurfaceVariant)),
                ],
                const SizedBox(width: 6),
                // 暂停/继续
                if (t.status == 'paused')
                  IconButton(
                    tooltip: '继续',
                    onPressed: onPauseResume,
                    icon: Icon(Icons.play_arrow_rounded,
                        size: 20, color: cs.primary),
                  )
                else if (running || t.status == 'queued')
                  IconButton(
                    tooltip: '暂停',
                    onPressed: onPauseResume,
                    icon: Icon(Icons.pause_rounded,
                        size: 20, color: cs.onSurfaceVariant),
                  ),
                // 取消
                if (running || t.status == 'queued' || t.status == 'paused')
                  IconButton(
                    tooltip: '取消任务',
                    onPressed: onCancel,
                    icon: Icon(Icons.stop_circle_outlined,
                        size: 20, color: cs.error.withValues(alpha: 0.85)),
                  ),
                // 打开文件夹
                if (t.status != 'queued')
                  IconButton(
                    tooltip: '打开文件夹',
                    onPressed: onOpenFolder,
                    icon: Icon(Icons.folder_open_rounded,
                        size: 20, color: cs.onSurfaceVariant),
                  ),
                // 浏览器预览
                if (t.status == 'done')
                  IconButton(
                    tooltip: '浏览器预览',
                    onPressed: onPreview,
                    icon: Icon(Icons.public_rounded, size: 20, color: cs.primary),
                  ),
                // 移除
                if (finished)
                  IconButton(
                    tooltip: '移除记录',
                    onPressed: onRemove,
                    icon: Icon(Icons.delete_outline_rounded,
                        size: 19, color: cs.outline),
                  ),
              ],
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              children: [
                _Meta(icon: Icons.play_circle_outline_rounded,
                    text: '开始 ${fmtTime(t.startedAtMs.toInt())}'),
                if (finished)
                  _Meta(icon: Icons.check_circle_outline_rounded,
                      text: '完成 ${fmtTime(t.finishedAtMs.toInt())}'),
                if (t.elapsedMs != BigInt.zero && (running || finished || t.status == 'paused'))
                  _Meta(icon: Icons.timer_outlined, text: '总用时 ${fmtDuration(t.elapsedMs.toInt())}'),
                if (t.status == 'queued')
                  _Meta(icon: Icons.hourglass_empty_rounded, text: '等待空闲槽位'),
                if (running || finished || t.status == 'paused') ...[
                  if (running) _Meta(icon: Icons.stairs_rounded, text: '当前 L${t.level}'),
                  _Meta(icon: Icons.grid_view_rounded,
                      text: 'Z${t.zmin ?? 0}–Z${t.zmax ?? '?'} · ${t.tilesDone} / ${t.totalTiles} 瓦片'),
                  if (eta != null)
                    _Meta(icon: Icons.schedule_rounded, text: '剩余约 $eta'),
                ],
                if ((speed ?? 0) > 0)
                  _Meta(icon: Icons.speed_rounded, text: '${fmtBytes(speed!)} /s'),
                if (t.bytesWritten != BigInt.zero)
                  _Meta(icon: Icons.save_rounded, text: fmtBytes(t.bytesWritten.toInt())),
                _Meta(icon: Icons.alt_route_rounded, text: schemeName(t.scheme)),
                _Meta(icon: Icons.opacity_rounded, text: alphaLabel(t.alpha)),
              ],
            ),
            if (t.error != null && t.error!.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(t.error!,
                  style: TextStyle(fontSize: 12, color: cs.error),
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis),
            ],
          ],
        ),
      ),
    );
  }

  IconData _statusIcon(String s) => switch (s) {
        'running' => Icons.autorenew_rounded,
        'paused' => Icons.pause_circle_outline_rounded,
        'done' => Icons.check_circle_rounded,
        'error' => Icons.error_rounded,
        'cancelled' => Icons.cancel_outlined,
        _ => Icons.schedule_rounded,
      };
}

// 状态徽章
class _StatusChip extends StatelessWidget {
  final String status;
  const _StatusChip({required this.status});

  @override
  Widget build(BuildContext context) {
    final c = statusColor(context, status);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: c.withValues(alpha: 0.13),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (status == 'running') ...[
            const SizedBox(
              width: 9,
              height: 9,
              child: CircularProgressIndicator(strokeWidth: 1.6),
            ),
            const SizedBox(width: 6),
          ],
          Text(statusLabel(status),
              style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: c)),
        ],
      ),
    );
  }
}

class _Meta extends StatelessWidget {
  final IconData icon;
  final String text;
  const _Meta({required this.icon, required this.text});

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: cs.surfaceContainerHighest.withValues(alpha: 0.45),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 13, color: cs.onSurfaceVariant),
          const SizedBox(width: 4),
          Text(text, style: TextStyle(fontSize: 12, color: cs.onSurfaceVariant)),
        ],
      ),
    );
  }
}
