import { ImagePlus, Trash2 } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import {
  useCustomEmojiQuery,
  useOwnCustomEmojiQuery,
  useRemoveCustomEmojiMutation,
  useSetCustomEmojiMutation,
} from "@/features/custom-emoji/hooks";
import { normalizeShortcode } from "@/shared/api/customEmoji";
import { pickAndUploadMedia } from "@/shared/api/tauri";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

/**
 * Custom emoji management (NIP-30, kind:30030). Each member owns their own set:
 * adding uploads an image and republishes the caller's own 30030; removing only
 * touches the caller's own set. So this card edits "My emoji" — the only set the
 * caller can publish — and shows the workspace palette (the read-only union of
 * every member's set) separately, since a member cannot remove someone else's
 * emoji. When shortcodes collide across members, the palette shows one
 * deterministic winner (see `unionCustomEmoji`).
 */
export function CustomEmojiSettingsCard() {
  const { data: own = [], isLoading: ownLoading } = useOwnCustomEmojiQuery();
  const { data: workspace = [], isLoading: workspaceLoading } =
    useCustomEmojiQuery();
  const setEmoji = useSetCustomEmojiMutation();
  const removeEmoji = useRemoveCustomEmojiMutation();

  const [name, setName] = React.useState("");
  const [isUploading, setIsUploading] = React.useState(false);

  const normalized = normalizeShortcode(name);
  const nameInvalid = name.trim().length > 0 && normalized === null;
  // "Replace" only applies to MY set — that's the set the upload will rewrite.
  const ownDuplicate =
    normalized !== null && own.some((e) => e.shortcode === normalized);
  const canSubmit = normalized !== null && !isUploading && !setEmoji.isPending;

  const handleAdd = React.useCallback(async () => {
    if (normalized === null) return;
    setIsUploading(true);
    try {
      const blobs = await pickAndUploadMedia();
      const url = blobs[0]?.url;
      if (!url) {
        // User cancelled the picker, or nothing uploaded.
        return;
      }
      const stored = await setEmoji.mutateAsync({ shortcode: normalized, url });
      setName("");
      toast.success(`Added :${stored}:`);
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to add emoji.",
      );
    } finally {
      setIsUploading(false);
    }
  }, [normalized, setEmoji]);

  const handleRemove = React.useCallback(
    async (shortcode: string) => {
      try {
        await removeEmoji.mutateAsync(shortcode);
        toast.success(`Removed :${shortcode}:`);
      } catch (error) {
        toast.error(
          error instanceof Error ? error.message : "Failed to remove emoji.",
        );
      }
    },
    [removeEmoji],
  );

  // Workspace emoji owned by someone else (so the caller can't remove them).
  const ownShortcodes = new Set(own.map((e) => e.shortcode));
  const othersEmoji = workspace.filter((e) => !ownShortcodes.has(e.shortcode));

  return (
    <section className="min-w-0 space-y-6" data-testid="settings-custom-emoji">
      <div className="space-y-1">
        <h2 className="text-sm font-semibold tracking-tight">Custom Emoji</h2>
        <p className="text-sm text-muted-foreground">
          Add your own custom emoji for everyone on this relay to use. Type{" "}
          <code>:name:</code> in messages and reactions.
        </p>
      </div>

      <form
        className="flex items-end gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit) void handleAdd();
        }}
      >
        <div className="min-w-0 flex-1 space-y-1.5">
          <label className="text-sm font-medium" htmlFor="custom-emoji-name">
            Name
          </label>
          <div className="flex items-center gap-1">
            <span className="text-muted-foreground">:</span>
            <Input
              id="custom-emoji-name"
              data-testid="custom-emoji-name-input"
              autoCapitalize="none"
              autoCorrect="off"
              placeholder="party-parrot"
              spellCheck={false}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
            <span className="text-muted-foreground">:</span>
          </div>
        </div>
        <Button
          type="submit"
          data-testid="custom-emoji-add"
          disabled={!canSubmit}
        >
          <ImagePlus className="mr-2 h-4 w-4" />
          {isUploading ? "Uploading…" : "Upload image"}
        </Button>
      </form>
      {nameInvalid ? (
        <p className="text-sm text-destructive">
          Use only letters, numbers, hyphen, or underscore.
        </p>
      ) : ownDuplicate ? (
        <p className="text-sm text-muted-foreground">
          You already have :{normalized}: — uploading will replace its image.
        </p>
      ) : null}

      <div className="space-y-3" data-testid="custom-emoji-mine">
        <h3 className="text-sm font-medium">
          My emoji{own.length > 0 ? ` (${own.length})` : ""}
        </h3>
        {ownLoading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : own.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            You haven&apos;t added any emoji yet. Add one above.
          </p>
        ) : (
          <ul className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
            {own.map((e) => (
              <li
                key={e.shortcode}
                className="flex items-center gap-3 rounded-lg border bg-card px-3 py-2"
              >
                <img
                  alt={`:${e.shortcode}:`}
                  src={rewriteRelayUrl(e.url)}
                  className="h-6 w-6 shrink-0 object-contain"
                  draggable={false}
                />
                <span className="min-w-0 flex-1 truncate text-sm">
                  :{e.shortcode}:
                </span>
                <Button
                  aria-label={`Remove :${e.shortcode}:`}
                  size="icon"
                  variant="ghost"
                  onClick={() => void handleRemove(e.shortcode)}
                  disabled={removeEmoji.isPending}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {!workspaceLoading && othersEmoji.length > 0 ? (
        <div className="space-y-3" data-testid="custom-emoji-workspace">
          <h3 className="text-sm font-medium">
            Workspace emoji ({othersEmoji.length})
          </h3>
          <p className="text-sm text-muted-foreground">
            Added by other members. You can use these, but only their owner can
            remove them.
          </p>
          <ul className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
            {othersEmoji.map((e) => (
              <li
                key={e.shortcode}
                className="flex items-center gap-3 rounded-lg border bg-card px-3 py-2"
              >
                <img
                  alt={`:${e.shortcode}:`}
                  src={rewriteRelayUrl(e.url)}
                  className="h-6 w-6 shrink-0 object-contain"
                  draggable={false}
                />
                <span className="min-w-0 flex-1 truncate text-sm">
                  :{e.shortcode}:
                </span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
