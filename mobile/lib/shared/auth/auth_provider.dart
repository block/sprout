import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../relay/relay.dart';
import '../workspace/workspace.dart';
import '../workspace/workspace_provider.dart';

enum AuthStatus { unknown, unauthenticated, authenticated, offline }

class AuthState {
  final AuthStatus status;
  final Workspace? workspace;

  const AuthState({required this.status, this.workspace});
}

/// Validates the active workspace on startup by opening a NIP-42-authenticated
/// websocket. A successful AUTH means the nsec is valid and the relay accepts
/// us; any other outcome falls through to offline (transient) or removes the
/// workspace (auth explicitly rejected).
class AuthNotifier extends AsyncNotifier<AuthState> {
  @override
  Future<AuthState> build() async {
    // Read from storage directly — NOT from workspace providers.
    // Watching workspace providers here would create a circular dependency
    // because authenticateWithWorkspace() writes to those providers.
    final storage = ref.read(workspaceStorageProvider);
    final workspaces = await storage.loadAll();
    if (workspaces.isEmpty) {
      return const AuthState(status: AuthStatus.unauthenticated);
    }

    final activeId = await storage.loadActiveId();
    final Workspace active;
    if (activeId != null && workspaces.any((w) => w.id == activeId)) {
      active = workspaces.firstWhere((w) => w.id == activeId);
    } else {
      // activeId is null or points to a workspace that no longer exists.
      // Fall back to first workspace and persist the choice.
      active = workspaces.first;
      await storage.saveActiveId(active.id);
    }

    // Validate by attempting a NIP-42 authenticated WS connection.
    final socket = RelaySocket(
      wsUrl: _wsFromBase(active.relayUrl),
      nsec: active.nsec,
      onMessage: (_) {},
      onConnected: () {},
      onDisconnected: (_) {},
    );
    try {
      await socket.connect().timeout(const Duration(seconds: 8));
      await socket.disconnect();
      return AuthState(status: AuthStatus.authenticated, workspace: active);
    } catch (e) {
      final msg = e.toString();
      // The relay explicitly rejected our auth — drop this workspace.
      if (msg.contains('Auth rejected') ||
          msg.contains('restricted') ||
          msg.contains('auth-required')) {
        await storage.remove(active.id);
        final remaining = await storage.loadAll();
        if (remaining.isNotEmpty) {
          final next = remaining.first;
          await storage.saveActiveId(next.id);
          ref.invalidate(workspaceListProvider);
          ref.invalidate(activeWorkspaceProvider);
          ref.invalidateSelf();
          return await future;
        }
        return const AuthState(status: AuthStatus.unauthenticated);
      }
      // Transient (timeout, network) — keep workspace, go offline.
      return AuthState(status: AuthStatus.offline, workspace: active);
    }
  }

  /// Retry credential validation (e.g. after a network error).
  Future<void> retry() async {
    ref.invalidateSelf();
    await future;
  }

  /// Authenticate with a workspace. Saves it and switches to it.
  /// Writes to storage directly to avoid circular dependency with workspace
  /// providers.
  Future<void> authenticateWithWorkspace(Workspace workspace) async {
    final storage = ref.read(workspaceStorageProvider);
    await storage.save(workspace);
    await storage.saveActiveId(workspace.id);

    // Invalidate workspace providers so other consumers pick up the new data.
    ref.invalidate(workspaceListProvider);
    ref.invalidate(activeWorkspaceProvider);

    state = AsyncData(
      AuthState(status: AuthStatus.authenticated, workspace: workspace),
    );
  }

  Future<void> signOut() async {
    final storage = ref.read(workspaceStorageProvider);
    final activeId = await storage.loadActiveId();
    if (activeId != null) {
      await storage.remove(activeId);
      await storage.clearActiveId();
    }

    // Check if other workspaces remain — switch to the next one instead of
    // forcing the user back to the pairing screen.
    final remaining = await storage.loadAll();

    // Invalidate workspace providers so other consumers pick up the change.
    ref.invalidate(workspaceListProvider);
    ref.invalidate(activeWorkspaceProvider);

    if (remaining.isNotEmpty) {
      final next = remaining.first;
      await storage.saveActiveId(next.id);
      // Re-run build() to validate the next workspace's credentials.
      ref.invalidateSelf();
      await future;
    } else {
      state = const AsyncData(AuthState(status: AuthStatus.unauthenticated));
    }
  }
}

/// Derive the websocket URL from the workspace's HTTP base URL.
String _wsFromBase(String baseUrl) {
  final uri = Uri.parse(baseUrl);
  final scheme = uri.scheme == 'https' ? 'wss' : 'ws';
  return uri.replace(scheme: scheme).toString();
}

final authProvider = AsyncNotifierProvider<AuthNotifier, AuthState>(
  AuthNotifier.new,
);
