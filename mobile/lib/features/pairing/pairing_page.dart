import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../../shared/theme/theme.dart';
import 'pairing_provider.dart';

class PairingPage extends HookConsumerWidget {
  const PairingPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pairingState = ref.watch(pairingProvider);
    final codeController = useTextEditingController();
    final isConnecting = pairingState.status == PairingStatus.connecting;

    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Grid.sm),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Spacer(flex: 2),

              // Branding
              Icon(LucideIcons.sprout, size: 64, color: context.colors.primary),
              const SizedBox(height: Grid.xs),
              Text('Welcome to Sprout', style: context.textTheme.headlineSmall),
              const SizedBox(height: Grid.xxs),
              Text(
                'Scan the QR code from your desktop app\nor paste a pairing code to connect.',
                textAlign: TextAlign.center,
                style: context.textTheme.bodyMedium?.copyWith(
                  color: context.colors.onSurfaceVariant,
                ),
              ),

              const SizedBox(height: Grid.lg),

              // Scan QR button
              FilledButton.icon(
                onPressed: isConnecting
                    ? null
                    : () => _openScanner(context, ref),
                icon: const Icon(LucideIcons.scanLine),
                label: const Text('Scan QR Code'),
              ),

              const SizedBox(height: Grid.sm),

              // Divider
              Row(
                children: [
                  const Expanded(child: Divider()),
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Grid.twelve,
                    ),
                    child: Text(
                      'or paste pairing code',
                      style: context.textTheme.bodySmall?.copyWith(
                        color: context.colors.onSurfaceVariant,
                      ),
                    ),
                  ),
                  const Expanded(child: Divider()),
                ],
              ),

              const SizedBox(height: Grid.sm),

              // Paste field
              TextField(
                controller: codeController,
                decoration: const InputDecoration(
                  hintText: 'sprout://...',
                  prefixIcon: Icon(LucideIcons.link),
                  isDense: true,
                ),
                autocorrect: false,
                enableSuggestions: false,
                enabled: !isConnecting,
                contextMenuBuilder: (context, editableTextState) {
                  return AdaptiveTextSelectionToolbar.editableText(
                    editableTextState: editableTextState,
                  );
                },
              ),

              const SizedBox(height: Grid.twelve),

              // Connect button
              SizedBox(
                width: double.infinity,
                child: FilledButton(
                  onPressed: isConnecting
                      ? null
                      : () {
                          final code = codeController.text.trim();
                          if (code.isNotEmpty) {
                            ref.read(pairingProvider.notifier).pair(code);
                          }
                        },
                  child: isConnecting
                      ? const SizedBox(
                          width: 20,
                          height: 20,
                          child: CircularProgressIndicator(
                            strokeWidth: 2,
                            color: Colors.white,
                          ),
                        )
                      : const Text('Connect'),
                ),
              ),

              // Error message
              if (pairingState.status == PairingStatus.error &&
                  pairingState.errorMessage != null) ...[
                const SizedBox(height: Grid.twelve),
                Container(
                  padding: const EdgeInsets.all(Grid.twelve),
                  decoration: BoxDecoration(
                    color: context.colors.errorContainer,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Row(
                    children: [
                      Icon(
                        LucideIcons.triangleAlert,
                        size: 16,
                        color: context.colors.onErrorContainer,
                      ),
                      const SizedBox(width: Grid.xxs),
                      Expanded(
                        child: Text(
                          pairingState.errorMessage!,
                          style: context.textTheme.bodySmall?.copyWith(
                            color: context.colors.onErrorContainer,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ],

              const Spacer(flex: 3),
            ],
          ),
        ),
      ),
    );
  }

  void _openScanner(BuildContext context, WidgetRef ref) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => _ScannerPage(
          onScanned: (code) {
            Navigator.of(context).pop();
            ref.read(pairingProvider.notifier).pair(code);
          },
        ),
      ),
    );
  }
}

class _ScannerPage extends StatefulWidget {
  final void Function(String code) onScanned;

  const _ScannerPage({required this.onScanned});

  @override
  State<_ScannerPage> createState() => _ScannerPageState();
}

class _ScannerPageState extends State<_ScannerPage> {
  bool _handled = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Scan QR Code'),
        leading: IconButton(
          icon: const Icon(LucideIcons.arrowLeft),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ),
      body: MobileScanner(
        onDetect: (capture) {
          if (_handled) return;
          final barcodes = capture.barcodes;
          if (barcodes.isNotEmpty) {
            final value = barcodes.first.rawValue;
            if (value != null && value.isNotEmpty) {
              _handled = true;
              widget.onScanned(value);
            }
          }
        },
      ),
    );
  }
}
