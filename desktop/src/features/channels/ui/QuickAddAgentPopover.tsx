import { Settings2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import * as React from "react";

import { Toggle } from "@/shared/ui/toggle";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Spinner } from "@/shared/ui/spinner";
import { getItemKey, useQuickAddAgentItems } from "./useQuickAddAgentItems";
import { useQuickAddAgentActions } from "./useQuickAddAgentActions";
import { QuickAddAgentItemRow } from "./QuickAddAgentItemRow";

// ── Component ─────────────────────────────────────────────────────────────────

type QuickAddAgentPopoverProps = {
  channelId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onMoreOptions: () => void;
  children: React.ReactNode;
};

export function QuickAddAgentPopover({
  channelId,
  open,
  onOpenChange,
  onMoreOptions,
  children,
}: QuickAddAgentPopoverProps) {
  const {
    items,
    isLoading,
    managedAgents,
    personas,
    providers,
    defaultProvider,
    usableTeams,
    pushRecent,
  } = useQuickAddAgentItems(channelId, open && channelId !== null);

  const {
    pendingKey,
    errorMessage,
    selectMode,
    setSelectMode,
    selectedKeys,
    selectedTeamIds,
    reset,
    handleCancelSelect,
    handleTeamToggle,
    handleBatchAdd,
    handleItemClick,
    removeMutation,
  } = useQuickAddAgentActions({
    channelId,
    items,
    managedAgents,
    personas,
    providers,
    defaultProvider,
    usableTeams,
    pushRecent,
  });

  // Reset state when popover closes
  React.useEffect(() => {
    if (!open) reset();
  }, [open, reset]);

  const multiSelectActive = selectMode && selectedKeys.size > 0;

  if (!channelId) {
    return <>{children}</>;
  }

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent
        align="end"
        className="w-72 overflow-hidden p-0"
        sideOffset={6}
      >
        {/* biome-ignore lint/a11y/useSemanticElements: composite widget with roving focus */}
        <div
          className="relative flex flex-col"
          role="group"
          aria-label="Add agent"
          onKeyDown={(e) => {
            const container = e.currentTarget;
            const buttons = Array.from(
              container.querySelectorAll<HTMLButtonElement>(
                "[data-quick-add-item]:not([disabled])",
              ),
            );
            if (buttons.length === 0) return;
            const focused = document.activeElement as HTMLElement | null;
            const currentIdx = focused
              ? buttons.indexOf(focused as HTMLButtonElement)
              : -1;

            if (e.key === "ArrowDown") {
              e.preventDefault();
              const next = currentIdx < buttons.length - 1 ? currentIdx + 1 : 0;
              buttons[next]?.focus();
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              const prev = currentIdx > 0 ? currentIdx - 1 : buttons.length - 1;
              buttons[prev]?.focus();
            }
          }}
        >
          {/* Header */}
          <div className="flex min-h-10 items-center gap-2 border-b pl-3 pr-1.5 py-1.5">
            <h3 className="shrink-0 text-sm font-semibold text-foreground">
              Add agent
            </h3>
            {usableTeams.length > 0 ? (
              <div className="ml-auto flex shrink-0 items-center">
                <Button
                  className="border border-input bg-transparent"
                  onClick={() => {
                    if (selectMode) handleCancelSelect();
                    else setSelectMode(true);
                  }}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  {selectMode ? "Cancel" : "Select"}
                </Button>
              </div>
            ) : null}
          </div>

          {/* Team toggles row */}
          <motion.div
            className="relative overflow-hidden"
            initial={false}
            animate={{
              height: selectMode ? "auto" : 0,
              opacity: selectMode ? 1 : 0,
            }}
            transition={{ duration: 0.2 }}
          >
            <div className="border-b px-3 pb-1 pt-0.5">
              <div className="flex items-center gap-1.5 overflow-x-auto">
                {usableTeams.map((team, index) => (
                  <motion.div
                    key={team.id}
                    initial={false}
                    animate={{
                      opacity: selectMode ? 1 : 0,
                      x: selectMode ? 0 : index === 0 ? 0 : 12,
                    }}
                    transition={{
                      duration: 0.2,
                      delay: selectMode ? index * 0.05 : 0,
                    }}
                    className="shrink-0"
                  >
                    <Toggle
                      className="h-6 rounded-full px-2.5 text-[11px]"
                      onPressedChange={(pressed) =>
                        handleTeamToggle(team, pressed)
                      }
                      pressed={selectedTeamIds.has(team.id)}
                      size="sm"
                      variant="subtle"
                    >
                      {team.name}
                    </Toggle>
                  </motion.div>
                ))}
              </div>
              <div className="pointer-events-none absolute right-0 top-0 h-full w-6 bg-gradient-to-l from-popover to-transparent" />
            </div>
          </motion.div>

          {/* Scrollable content */}
          <motion.div
            className="max-h-[13.75rem] flex-1 overflow-y-auto"
            layout
            transition={{ duration: 0.2 }}
          >
            {isLoading ? (
              <div className="flex items-center justify-center py-6">
                <Spinner className="h-4 w-4 text-muted-foreground" />
              </div>
            ) : items.length === 0 ? (
              <div className="px-3 py-4 text-center text-sm text-muted-foreground">
                No agents available.
              </div>
            ) : (
              <div
                aria-label="Available agents"
                className="py-1"
                role="listbox"
              >
                {items.map((item) => {
                  const itemKey = getItemKey(item);
                  return (
                    <QuickAddAgentItemRow
                      key={itemKey}
                      item={item}
                      itemKey={itemKey}
                      isSelected={selectedKeys.has(itemKey)}
                      isPending={
                        pendingKey === itemKey || pendingKey === "batch"
                      }
                      selectMode={selectMode}
                      disabled={Boolean(pendingKey)}
                      onClick={() => handleItemClick(item)}
                      onRemove={(pubkey) => removeMutation.mutate(pubkey)}
                    />
                  );
                })}
              </div>
            )}
          </motion.div>

          {errorMessage ? (
            <div className="border-t px-3 py-2">
              <p className="text-xs text-destructive">{errorMessage}</p>
            </div>
          ) : null}

          <div className="border-t">
            <button
              className="flex w-full items-center gap-2 px-3 py-2.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              data-quick-add-item
              data-testid="quick-add-more-options"
              onClick={() => {
                onOpenChange(false);
                onMoreOptions();
              }}
              type="button"
            >
              <Settings2 className="h-3.5 w-3.5" />
              <span>More options…</span>
            </button>
          </div>

          <AnimatePresence>
            {multiSelectActive ? (
              <motion.div
                key="batch-add"
                className="pointer-events-none absolute bottom-0 left-0 right-0 px-1 pb-1 pt-8 bg-gradient-to-t from-popover to-transparent"
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 8 }}
                transition={{ duration: 0.15 }}
              >
                <Button
                  className="pointer-events-auto w-full"
                  data-testid="quick-add-batch-confirm"
                  disabled={Boolean(pendingKey)}
                  onClick={() => void handleBatchAdd()}
                  size="sm"
                  type="button"
                >
                  {pendingKey === "batch" ? (
                    <Spinner className="h-3 w-3" />
                  ) : null}
                  Add ({selectedKeys.size})
                </Button>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
      </PopoverContent>
    </Popover>
  );
}
