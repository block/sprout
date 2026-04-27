import { Check, ChevronDown, Pencil, Plus, Trash2 } from "lucide-react";
import * as React from "react";

import type { Workspace } from "@/features/workspaces/types";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/shared/ui/sidebar";

type WorkspaceSwitcherProps = {
  activeWorkspace: Workspace | null;
  workspaces: Workspace[];
  onSwitchWorkspace: (id: string) => void;
  onAddWorkspace: () => void;
  onRenameWorkspace: (id: string, name: string) => void;
  onRemoveWorkspace: (id: string) => void;
};

export function WorkspaceSwitcher({
  activeWorkspace,
  workspaces,
  onSwitchWorkspace,
  onAddWorkspace,
  onRenameWorkspace,
  onRemoveWorkspace,
}: WorkspaceSwitcherProps) {
  const [renamingId, setRenamingId] = React.useState<string | null>(null);
  const [renameValue, setRenameValue] = React.useState("");

  const handleStartRename = React.useCallback(
    (e: React.MouseEvent, workspace: Workspace) => {
      e.stopPropagation();
      setRenamingId(workspace.id);
      setRenameValue(workspace.name);
    },
    [],
  );

  const handleFinishRename = React.useCallback(() => {
    if (renamingId && renameValue.trim()) {
      onRenameWorkspace(renamingId, renameValue.trim());
    }
    setRenamingId(null);
    setRenameValue("");
  }, [renamingId, renameValue, onRenameWorkspace]);

  const handleRenameKeyDown = React.useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        handleFinishRename();
      } else if (e.key === "Escape") {
        setRenamingId(null);
        setRenameValue("");
      }
    },
    [handleFinishRename],
  );

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton
              className="h-auto gap-2 rounded-xl px-2.5 py-2 data-[state=open]:bg-sidebar-accent"
              data-testid="workspace-switcher"
              type="button"
            >
              <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-primary/15 text-[10px] font-bold text-primary">
                {activeWorkspace?.name?.[0]?.toUpperCase() ?? "W"}
              </span>
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {activeWorkspace?.name ?? "No workspace"}
              </span>
              <ChevronDown className="h-3.5 w-3.5 shrink-0 text-sidebar-foreground/50" />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            className="w-[--radix-dropdown-menu-trigger-width] min-w-[220px]"
            side="top"
            sideOffset={4}
          >
            {workspaces.map((workspace) => (
              <DropdownMenuItem
                key={workspace.id}
                className="group flex items-center gap-2 pr-1"
                onSelect={(e) => {
                  if (renamingId === workspace.id) {
                    e.preventDefault();
                    return;
                  }
                  onSwitchWorkspace(workspace.id);
                }}
              >
                {renamingId === workspace.id ? (
                  <input
                    className="flex-1 bg-transparent text-sm outline-none"
                    onBlur={handleFinishRename}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={handleRenameKeyDown}
                    ref={(el) => el?.focus()}
                    value={renameValue}
                  />
                ) : (
                  <>
                    <span className="flex h-4 w-4 shrink-0 items-center justify-center">
                      {activeWorkspace?.id === workspace.id ? (
                        <Check className="h-3.5 w-3.5 text-primary" />
                      ) : null}
                    </span>
                    <span className="min-w-0 flex-1 truncate">
                      {workspace.name}
                    </span>
                    <div className="flex shrink-0 items-center opacity-0 group-hover:opacity-100 group-focus:opacity-100">
                      <button
                        aria-label={`Rename ${workspace.name}`}
                        className="rounded p-0.5 hover:bg-accent"
                        onClick={(e) => handleStartRename(e, workspace)}
                        type="button"
                      >
                        <Pencil className="h-3 w-3" />
                      </button>
                      {workspaces.length > 1 ? (
                        <button
                          aria-label={`Remove ${workspace.name}`}
                          className="rounded p-0.5 hover:bg-destructive/20 hover:text-destructive"
                          onClick={(e) => {
                            e.stopPropagation();
                            onRemoveWorkspace(workspace.id);
                          }}
                          type="button"
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      ) : null}
                    </div>
                  </>
                )}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={onAddWorkspace}>
              <Plus className="h-4 w-4" />
              <span>Add Workspace</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
