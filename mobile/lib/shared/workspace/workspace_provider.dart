import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../auth/auth_provider.dart';
import 'workspace.dart';
import 'workspace_storage.dart';

final workspaceStorageProvider = Provider<WorkspaceStorage>((ref) {
  return WorkspaceStorage();
});

class WorkspaceListNotifier extends AsyncNotifier<List<Workspace>> {
  @override
  Future<List<Workspace>> build() async {
    final storage = ref.read(workspaceStorageProvider);
    return storage.loadAll();
  }

  /// Add a workspace. If one with the same relay URL already exists, update
  /// its credentials instead. Returns the effective workspace ID.
  Future<String> addWorkspace(Workspace workspace) async {
    final storage = ref.read(workspaceStorageProvider);
    final current = state.value ?? [];

    // If a workspace with the same relay URL exists, update its credentials
    // instead of creating a duplicate entry.
    final existingIndex = current.indexWhere(
      (w) => w.relayUrl == workspace.relayUrl,
    );
    if (existingIndex >= 0) {
      final existing = current[existingIndex];
      final updated = existing.copyWith(
        pubkey: workspace.pubkey,
        nsec: workspace.nsec,
      );
      await storage.save(updated);
      final updatedList = [...current];
      updatedList[existingIndex] = updated;
      state = AsyncData(updatedList);
      return existing.id;
    }

    await storage.save(workspace);
    state = AsyncData([...current, workspace]);
    return workspace.id;
  }

  Future<void> removeWorkspace(String id) async {
    final storage = ref.read(workspaceStorageProvider);
    await storage.remove(id);

    final current = state.value ?? [];
    state = AsyncData(current.where((w) => w.id != id).toList());

    // If we removed the active workspace, switch to another or sign out.
    final activeId = await storage.loadActiveId();
    if (activeId == id) {
      final remaining = state.value ?? [];
      if (remaining.isNotEmpty) {
        await switchWorkspace(remaining.first.id);
      } else {
        await storage.clearActiveId();
        // Invalidate auth so it re-evaluates against the now-empty storage
        // and transitions to unauthenticated.
        ref.invalidate(authProvider);
      }
    }
  }

  Future<void> switchWorkspace(String id) async {
    final storage = ref.read(workspaceStorageProvider);
    await storage.saveActiveId(id);
    // Reassign list state to trigger activeWorkspaceProvider (which watches
    // workspaceListProvider.future) to rebuild and pick up the new active ID.
    // We can't use ref.invalidate(activeWorkspaceProvider) here because that
    // creates a circular dependency — activeWorkspaceProvider watches us.
    state = AsyncData([...state.value ?? []]);
    // Invalidate auth so AuthState.workspace reflects the new active workspace.
    ref.invalidate(authProvider);
  }

  Future<void> renameWorkspace(String id, String name) async {
    final storage = ref.read(workspaceStorageProvider);
    final current = state.value ?? [];
    final index = current.indexWhere((w) => w.id == id);
    if (index < 0) return;

    final updated = current[index].copyWith(name: name);
    await storage.save(updated);

    final updatedList = [...current];
    updatedList[index] = updated;
    state = AsyncData(updatedList);
  }
}

final workspaceListProvider =
    AsyncNotifierProvider<WorkspaceListNotifier, List<Workspace>>(
      WorkspaceListNotifier.new,
    );

/// The currently active workspace, derived from the stored active ID and
/// the workspace list.
final activeWorkspaceProvider = FutureProvider<Workspace?>((ref) async {
  final workspaces = await ref.watch(workspaceListProvider.future);
  final storage = ref.read(workspaceStorageProvider);
  final activeId = await storage.loadActiveId();

  if (workspaces.isEmpty) return null;

  if (activeId == null) {
    // No active ID stored but workspaces exist — fall back to first.
    await storage.saveActiveId(workspaces.first.id);
    return workspaces.first;
  }

  try {
    return workspaces.firstWhere((w) => w.id == activeId);
  } on StateError {
    // Active ID points to a workspace that no longer exists.
    // Fall back to first workspace.
    if (workspaces.isNotEmpty) {
      await storage.saveActiveId(workspaces.first.id);
      return workspaces.first;
    }
    return null;
  }
});
