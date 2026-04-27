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

    // Validate the active workspace credentials against the relay.
    final client = RelayClient(
      baseUrl: active.relayUrl,
      apiToken: active.token,
    );
    try {
      await client.get('/api/users/me/profile');
      return AuthState(status: AuthStatus.authenticated, workspace: active);
    } on RelayException catch (e) {
      if (e.statusCode == 401 || e.statusCode == 403) {
        // Token is invalid or revoked — remove from storage directly.
        await storage.remove(active.id);
        return const AuthState(status: AuthStatus.unauthenticated);
      }
      // Transient server error (5xx, 429, etc.) — keep workspace, go offline.
      return AuthState(status: AuthStatus.offline, workspace: active);
    } catch (_) {
      // Network error — keep workspace and let the user retry.
      return AuthState(status: AuthStatus.offline, workspace: active);
    } finally {
      client.dispose();
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

final authProvider = AsyncNotifierProvider<AuthNotifier, AuthState>(
  AuthNotifier.new,
);
