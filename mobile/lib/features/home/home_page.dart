import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../activity/activity_page.dart';
import '../channels/channels_page.dart';

class HomePage extends HookConsumerWidget {
  const HomePage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tabIndex = useState(0);

    const pages = [ChannelsPage(), ActivityPage()];

    return Scaffold(
      body: IndexedStack(index: tabIndex.value, children: pages),
      bottomNavigationBar: NavigationBar(
        selectedIndex: tabIndex.value,
        onDestinationSelected: (i) => tabIndex.value = i,
        destinations: const [
          NavigationDestination(
            icon: Icon(LucideIcons.house),
            selectedIcon: Icon(LucideIcons.house),
            label: 'Home',
          ),
          NavigationDestination(
            icon: Icon(LucideIcons.bell),
            selectedIcon: Icon(LucideIcons.bell),
            label: 'Activity',
          ),
        ],
      ),
    );
  }
}
