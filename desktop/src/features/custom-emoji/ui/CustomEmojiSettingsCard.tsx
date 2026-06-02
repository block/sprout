import { ImagePlus, Trash2 } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import {
  useCustomEmojiQuery,
  useRemoveCustomEmojiMutation,
  useSetCustomEmojiMutation,
} from "@/features/custom-emoji/hooks";
import { normalizeShortcode } from "@/shared/api/customEmoji";
import { pickAndUploadMedia } from "@/shared/api/tauri";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

/**
 * Relay-wide custom emoji management (Slack-style). Any relay member can add
 * (upload image + name) or remove emoji; the relay owns the canonical set.
 * Adds emit a kind:9037 command — the relay validates membership and re-signs
 * the kind:30030 set, which `useCustomEmojiQuery` then reflects.
 */
export function CustomEmojiSettingsCard() {
  const { data: emoji = [], isLoading } = useCustomEmojiQuery();
  const setEmoji = useSetCustomEmojiMutation();
  const removeEmoji = useRemoveCustomEmojiMutation();

  const [name, setName] = React.useState("");
  const [isUploading, setIsUploading] = React.useState(false);

  const normalized = normalizeShortcode(name);
  const nameInvalid = name.trim().length > 0 && normalized === null;
  const duplicate =
    normalized !== null && emoji.some((e) => e.shortcode === normalized);
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

  return (
    <section className="min-w-0 space-y-6" data-testid="settings-custom-emoji">
      <div className="space-y-1">
        <h2 className="text-sm font-semibold tracking-tight">Custom Emoji</h2>
        <p className="text-sm text-muted-foreground">
          Add custom emoji for everyone on this relay. Use them in messages and
          reactions by typing <code>:name:</code>.
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
      ) : duplicate ? (
        <p className="text-sm text-muted-foreground">
          :{normalized}: already exists — uploading will replace its image.
        </p>
      ) : null}

      <div className="space-y-3">
        <h3 className="text-sm font-medium">
          {emoji.length} custom {emoji.length === 1 ? "emoji" : "emoji"}
        </h3>
        {isLoading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : emoji.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No custom emoji yet. Add one above.
          </p>
        ) : (
          <ul className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
            {emoji.map((e) => (
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
    </section>
  );
}
