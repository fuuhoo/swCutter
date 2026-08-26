import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:url_launcher/url_launcher.dart';

import '../state/app_state.dart';
import '../widgets/task_card.dart';
import '../src/rust/api/task_api.dart' as rust;
import '../src/rust/api/preview_server.dart' as pserver;

/// 任务中心：所有切片任务的进度与操作。
class TasksPage extends ConsumerWidget {
  final VoidCallback onGoNewTask;
  const TasksPage({super.key, required this.onGoNewTask});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appProvider);
    final tasks = state.tasks.reversed.toList();
    final running = state
        .tasks
        .where((t) => t.status == 'running' || t.status == 'paused')
        .length;
    final queued = state.tasks.where((t) => t.status == 'queued').length;

    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text('任务中心',
                  style: Theme.of(context)
                      .textTheme
                      .headlineSmall
                      ?.copyWith(fontWeight: FontWeight.w700)),
              const SizedBox(width: 10),
              _Pill(text: '共 ${tasks.length}'),
              const SizedBox(width: 6),
              _Pill(text: '进行中 $running', accent: true),
              if (queued > 0) ...[
                const SizedBox(width: 6),
                _Pill(text: '排队 $queued'),
              ],
            ],
          ),
          const SizedBox(height: 16),
          if (tasks.isEmpty)
            Expanded(
              child: Center(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.layers_clear_rounded,
                        size: 72, color: Theme.of(context).colorScheme.outline),
                    const SizedBox(height: 12),
                    Text('还没有任务',
                        style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 6),
                    Text('选择一个 TIFF 文件开始你的第一次金字塔切片',
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                            color: Theme.of(context).colorScheme.outline)),
                    const SizedBox(height: 18),
                    FilledButton.icon(
                      onPressed: onGoNewTask,
                      icon: const Icon(Icons.add_rounded),
                      label: const Text('新建任务'),
                    ),
                  ],
                ),
              ),
            )
          else
            Expanded(
              child: ListView.separated(
                itemCount: tasks.length,
                separatorBuilder: (_, _) => const SizedBox(height: 12),
                itemBuilder: (context, i) {
                  final t = tasks[i];
                  return TaskCard(
                    task: t,
                    speedBps: state.speedBps[t.id.toInt()],
                    onCancel: () async {
                      try {
                        await rust.cancelTask(id: t.id);
                      } catch (e) {
                        if (context.mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(content: Text('取消失败: $e')));
                        }
                      }
                    },
                    onPauseResume: () async {
                      try {
                        if (t.status == 'paused') {
                          await rust.resumeTask(id: t.id);
                        } else {
                          await rust.pauseTask(id: t.id);
                        }
                      } catch (e) {
                        if (context.mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(content: Text('操作失败: $e')));
                        }
                      }
                    },
                    onOpenFolder: () => _openFolder(t.output),
                    onPreview: () => _openPreview(context, t.output),
                    onRemove: () async {
                      try {
                        final ok = await rust.removeTask(id: t.id);
                        if (ok) {
                          state.tasks.removeWhere((x) => x.id == t.id);
                          state.notifySelf();
                        }
                      } catch (_) {}
                    },
                  );
                },
              ),
            ),
        ],
      ),
    );
  }

  Future<void> _openFolder(String path) async {
    try {
      if (Directory(path).existsSync()) {
        Process.run('explorer.exe', [path]);
      }
    } catch (_) {}
  }

  Future<void> _openPreview(BuildContext context, String output) async {
    final sep = Platform.pathSeparator;
    final html = '$output${sep}preview.html';
    if (!File(html).existsSync()) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(
          content: Text('未找到 preview.html：$html'),
          backgroundColor: Theme.of(context).colorScheme.error));
      return;
    }
    try {
      // 内置静态服务（http://127.0.0.1 随机端口），绕开 file:// 唯一源限制
      final port = await pserver.previewServe(dir: output);
      await launchUrl(
        Uri.parse('http://127.0.0.1:$port/'),
        mode: LaunchMode.externalApplication,
      );
    } catch (e) {
      // 服务失败时回退 file:// 直接打开
      await launchUrl(Uri.file(html), mode: LaunchMode.externalApplication);
    }
  }
}

class _Pill extends StatelessWidget {
  final String text;
  final bool accent;
  const _Pill({required this.text, this.accent = false});

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: accent ? cs.primary.withValues(alpha: 0.15) : cs.surfaceContainerHighest.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        text,
        style: TextStyle(
            fontSize: 12,
            fontWeight: FontWeight.w600,
            color: accent ? cs.primary : cs.onSurfaceVariant),
      ),
    );
  }
}
