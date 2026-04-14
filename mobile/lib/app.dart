import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'features/home/home_page.dart';
import 'features/pairing/pairing_page.dart';
import 'shared/auth/auth.dart';
import 'shared/theme/theme.dart';

class App extends HookConsumerWidget {
  const App({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeProvider);
    final authState = ref.watch(authProvider);

    return MaterialApp(
      title: 'Sprout',
      theme: AppTheme.lightTheme,
      darkTheme: AppTheme.darkTheme,
      themeMode: themeMode,
      home: authState.when(
        loading: () => const _SplashScreen(),
        error: (_, _) => const PairingPage(),
        data: (state) => switch (state.status) {
          AuthStatus.authenticated => const HomePage(),
          AuthStatus.offline => const _OfflineScreen(),
          _ => const PairingPage(),
        },
      ),
    );
  }
}

class _SplashScreen extends StatelessWidget {
  const _SplashScreen();

  @override
  Widget build(BuildContext context) {
    return const Scaffold(body: Center(child: CircularProgressIndicator()));
  }
}

class _OfflineScreen extends ConsumerWidget {
  const _OfflineScreen();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Scaffold(
      body: Center(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Grid.sm),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                LucideIcons.wifiOff,
                size: 48,
                color: context.colors.onSurfaceVariant,
              ),
              const SizedBox(height: Grid.xs),
              Text(
                'Unable to reach relay',
                style: context.textTheme.titleMedium,
              ),
              const SizedBox(height: Grid.xxs),
              Text(
                'Your pairing is saved — check your connection and try again.',
                textAlign: TextAlign.center,
                style: context.textTheme.bodyMedium?.copyWith(
                  color: context.colors.onSurfaceVariant,
                ),
              ),
              const SizedBox(height: Grid.sm),
              FilledButton.icon(
                onPressed: () => ref.read(authProvider.notifier).retry(),
                icon: const Icon(LucideIcons.refreshCw),
                label: const Text('Retry'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
