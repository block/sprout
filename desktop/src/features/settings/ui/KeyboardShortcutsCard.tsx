import { Keyboard } from "lucide-react";

import {
  getShortcutsByCategory,
  getPlatformKeys,
  type KeyboardShortcut,
} from "@/shared/lib/keyboard-shortcuts";

function KeyCombo({ shortcut }: { shortcut: KeyboardShortcut }) {
  const keys = getPlatformKeys(shortcut);
  // Split on "+" but keep "+" as a standalone key (e.g. for zoom-in "⌘+")
  const parts = keys
    .split(/(?<!\+)\+(?!\s*$)/)
    .map((p) => p.trim())
    .filter(Boolean);

  return (
    <span className="flex items-center gap-1">
      {parts.map((part) => (
        <kbd
          className="inline-flex h-6 min-w-6 items-center justify-center rounded border border-border/70 bg-muted/60 px-1.5 font-mono text-xs text-muted-foreground"
          key={part}
        >
          {part}
        </kbd>
      ))}
    </span>
  );
}

export function KeyboardShortcutsCard() {
  const categories = getShortcutsByCategory();

  return (
    <section className="min-w-0" data-testid="settings-shortcuts">
      <div className="mb-3 min-w-0">
        <div className="flex items-center gap-2">
          <Keyboard className="h-4 w-4 text-muted-foreground" />
          <h2 className="text-sm font-semibold tracking-tight">
            Keyboard Shortcuts
          </h2>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          All available keyboard shortcuts. Shortcuts are read-only.
        </p>
      </div>

      <div className="space-y-4">
        {[...categories.entries()].map(([category, shortcuts]) => (
          <div key={category}>
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-widest text-muted-foreground">
              {category}
            </h3>
            <div className="rounded-lg border border-border/70 bg-background/70">
              {shortcuts.map((shortcut, i) => (
                <div
                  className={`flex items-center justify-between px-3 py-2 text-sm ${i !== shortcuts.length - 1 ? "border-b border-border/50" : ""}`}
                  key={shortcut.id}
                >
                  <div className="min-w-0 flex-1">
                    <span className="text-foreground">{shortcut.label}</span>
                    <span className="ml-2 text-muted-foreground">
                      {shortcut.description}
                    </span>
                  </div>
                  <KeyCombo shortcut={shortcut} />
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
