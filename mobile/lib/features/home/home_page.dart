import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../channels/channels_page.dart';
import '../settings/settings_page.dart';

class HomePage extends HookConsumerWidget {
  const HomePage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tabIndex = useState(0);

    const pages = [ChannelsPage(), SettingsPage()];

    return Scaffold(
      body: IndexedStack(index: tabIndex.value, children: pages),
      bottomNavigationBar: NavigationBar(
        selectedIndex: tabIndex.value,
        onDestinationSelected: (i) => tabIndex.value = i,
        destinations: const [
          NavigationDestination(
            icon: Icon(LucideIcons.hash),
            selectedIcon: Icon(LucideIcons.hash),
            label: 'Channels',
          ),
          NavigationDestination(
            icon: Icon(LucideIcons.settings),
            selectedIcon: Icon(LucideIcons.settings),
            label: 'Settings',
          ),
        ],
      ),
    );
  }
}
