import { Check, ChevronDown, MoreHorizontal, Plus } from "lucide-react";
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

import { EditWorkspaceDialog } from "./EditWorkspaceDialog";

type WorkspaceSwitcherProps = {
  activeWorkspace: Workspace | null;
  workspaces: Workspace[];
  onSwitchWorkspace: (id: string) => void;
  onAddWorkspace: () => void;
  onUpdateWorkspace: (
    id: string,
    updates: Partial<Pick<Workspace, "name" | "relayUrl" | "token">>,
  ) => void;
  onRemoveWorkspace: (id: string) => void;
};

export function WorkspaceSwitcher({
  activeWorkspace,
  workspaces,
  onSwitchWorkspace,
  onAddWorkspace,
  onUpdateWorkspace,
  onRemoveWorkspace,
}: WorkspaceSwitcherProps) {
  const [editingWorkspace, setEditingWorkspace] =
    React.useState<Workspace | null>(null);
  const [dropdownOpen, setDropdownOpen] = React.useState(false);

  return (
    <>
      <SidebarMenu>
        <SidebarMenuItem>
          <DropdownMenu open={dropdownOpen} onOpenChange={setDropdownOpen}>
            <DropdownMenuTrigger asChild>
              <SidebarMenuButton
                className="h-auto gap-2 rounded-xl px-2.5 py-2 data-[state=open]:bg-sidebar-accent"
                data-testid="workspace-switcher"
                type="button"
              >
                <span className="flex h-5 w-5 shrink-0 items-center justify-center text-xs leading-none">
                  🌱
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
              onCloseAutoFocus={(e) => e.preventDefault()}
              side="bottom"
              sideOffset={4}
            >
              {workspaces.map((workspace) => (
                <DropdownMenuItem
                  key={workspace.id}
                  className="group flex items-center gap-2 pr-1"
                  onSelect={() => {
                    onSwitchWorkspace(workspace.id);
                  }}
                >
                  <span className="flex h-4 w-4 shrink-0 items-center justify-center">
                    {activeWorkspace?.id === workspace.id ? (
                      <Check className="h-3.5 w-3.5 text-primary" />
                    ) : null}
                  </span>
                  <span className="min-w-0 flex-1 truncate">
                    {workspace.name}
                  </span>
                  <button
                    aria-label={`Edit ${workspace.name}`}
                    className="flex h-5 w-5 shrink-0 items-center justify-center rounded opacity-0 hover:bg-accent group-hover:opacity-100 group-focus:opacity-100"
                    onClick={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                      setDropdownOpen(false);
                      setEditingWorkspace(workspace);
                    }}
                    type="button"
                  >
                    <MoreHorizontal className="h-3.5 w-3.5" />
                  </button>
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

      <EditWorkspaceDialog
        canRemove={workspaces.length > 1}
        onOpenChange={(open) => {
          if (!open) setEditingWorkspace(null);
        }}
        onRemove={onRemoveWorkspace}
        onSave={onUpdateWorkspace}
        open={editingWorkspace !== null}
        workspace={editingWorkspace}
      />
    </>
  );
}
