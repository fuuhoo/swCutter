import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state/app_state.dart';
import '../version.dart';
import 'new_task_page.dart';
import 'settings_page.dart';
import 'tasks_page.dart';

/// 左侧导航 + 内容区的应用外壳。
class HomeShell extends ConsumerStatefulWidget {
  const HomeShell({super.key});

  @override
  ConsumerState<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends ConsumerState<HomeShell> {
  int _index = 0;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final runningCount = ref
        .watch(appProvider)
        .tasks
        .where((t) => t.status == 'running' || t.status == 'queued')
        .length;

    return Scaffold(
      body: Row(
        children: [
          NavigationRail(
            selectedIndex: _index,
            onDestinationSelected: (i) => setState(() => _index = i),
            extended: MediaQuery.of(context).size.width > 1100,
            minExtendedWidth: 190,
            backgroundColor:
                scheme.surface.withValues(alpha: 0.6),
            leading: Padding(
              padding: const EdgeInsets.symmetric(vertical: 18),
              child: Column(
                children: [
                  Container(
                    width: 44,
                    height: 44,
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        colors: [scheme.primary, scheme.tertiary],
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                      ),
                      borderRadius: BorderRadius.circular(14),
                    ),
                    child: const Icon(Icons.grid_on_rounded, color: Colors.white),
                  ),
                  const SizedBox(height: 8),
                  const Text('swCutter',
                      style: TextStyle(fontWeight: FontWeight.w700, fontSize: 15)),
                  Text('v$kAppVersion',
                      style: TextStyle(
                          fontSize: 10.5, color: scheme.outline)),
                ],
              ),
            ),
            destinations: [
              const NavigationRailDestination(
                icon: Icon(Icons.add_circle_outline_rounded),
                selectedIcon: Icon(Icons.add_circle_rounded),
                label: Text('新建任务'),
              ),
              const NavigationRailDestination(
                icon: Icon(Icons.queue_rounded),
                selectedIcon: Icon(Icons.queue_rounded),
                label: Text('任务中心'),
              ),
              const NavigationRailDestination(
                icon: Icon(Icons.settings_outlined),
                selectedIcon: Icon(Icons.settings_rounded),
                label: Text('设置'),
              ),
            ],
          ),
          VerticalDivider(width: 1, color: scheme.outlineVariant.withValues(alpha: 0.4)),
          Expanded(
            child: SwitchView(
              index: _index,
              runningCount: runningCount,
              onGoTasks: () => setState(() => _index = 1),
            ),
          ),
        ],
      ),
    );
  }
}

class SwitchView extends StatelessWidget {
  final int index;
  final int runningCount;
  final VoidCallback onGoTasks;
  const SwitchView({
    super.key,
    required this.index,
    required this.runningCount,
    required this.onGoTasks,
  });

  @override
  Widget build(BuildContext context) {
    switch (index) {
      case 1:
        return TasksPage(onGoNewTask: onGoTasks);
      case 2:
        return const SettingsPage();
      default:
        return const NewTaskPage();
    }
  }
}
