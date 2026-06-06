import { ImagePlus, Trash2 } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import {
  useCustomEmojiQuery,
  useOwnCustomEmojiQuery,
  useRemoveCustomEmojiMutation,
  useSetCustomEmojiMutation,
} from "@/features/custom-emoji/hooks";
import {
  normalizeShortcode,
  suggestShortcodeFromFilename,
} from "@/shared/api/customEmoji";
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
  const [pendingUpload, setPendingUpload] = React.useState<{
    url: string;
    filename: string | null;
  } | null>(null);
  const [isUploading, setIsUploading] = React.useState(false);

  const normalized = normalizeShortcode(name);
  const nameInvalid = name.trim().length > 0 && normalized === null;
  // "Replace" only applies to MY set — that's the set the upload will rewrite.
  const ownDuplicate =
    normalized !== null && own.some((e) => e.shortcode === normalized);
  const canSubmit =
    pendingUpload !== null &&
    normalized !== null &&
    !isUploading &&
    !setEmoji.isPending;

  const handleUpload = React.useCallback(async () => {
    setIsUploading(true);
    try {
      const blobs = await pickAndUploadMedia();
      const blob = blobs[0];
      if (!blob?.url) {
        return;
      }
      if (!blob.type.startsWith("image/")) {
        toast.error("Choose an image file for custom emoji.");
        return;
      }
      setPendingUpload({ url: blob.url, filename: blob.filename ?? null });
      const suggested = blob.filename
        ? suggestShortcodeFromFilename(blob.filename)
        : null;
      if (suggested && name.trim().length === 0) {
        setName(suggested);
      }
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : "Failed to upload emoji image.",
      );
    } finally {
      setIsUploading(false);
    }
  }, [name]);

  const handleAdd = React.useCallback(async () => {
    if (normalized === null || pendingUpload === null) return;
    try {
      const stored = await setEmoji.mutateAsync({
        shortcode: normalized,
        url: pendingUpload.url,
      });
      setName("");
      setPendingUpload(null);
      toast.success(`Added :${stored}:`);
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to add emoji.",
      );
    }
  }, [normalized, pendingUpload, setEmoji]);

  const handleReset = React.useCallback(() => {
    setName("");
    setPendingUpload(null);
  }, []);

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
        className="max-w-2xl space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit) void handleAdd();
        }}
      >
        <div className="space-y-3">
          <div>
            <h4 className="text-sm font-semibold">1. Upload an image</h4>
            <p className="text-sm text-muted-foreground">
              Square images work best. GIF, PNG, JPEG, and WebP files are
              supported.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-4">
            <div className="flex h-16 w-16 shrink-0 items-center justify-center rounded-md border bg-background">
              {pendingUpload ? (
                <img
                  alt="Selected custom emoji preview"
                  src={rewriteRelayUrl(pendingUpload.url)}
                  className="h-14 w-14 object-contain"
                  draggable={false}
                />
              ) : (
                <ImagePlus className="h-6 w-6 text-muted-foreground" />
              )}
            </div>
            <div className="min-w-0 flex-1 space-y-2">
              <p className="truncate text-sm text-muted-foreground">
                {pendingUpload?.filename ?? "No image selected"}
              </p>
              <Button
                type="button"
                data-testid="custom-emoji-upload"
                onClick={() => void handleUpload()}
                disabled={isUploading || setEmoji.isPending}
                variant="outline"
              >
                {isUploading
                  ? "Uploading…"
                  : pendingUpload
                    ? "Choose different image"
                    : "Upload image"}
              </Button>
            </div>
          </div>
        </div>

        <div className="space-y-3 border-t pt-4">
          <div>
            <h4 className="text-sm font-semibold">2. Give it a name</h4>
            <p className="text-sm text-muted-foreground">
              This is what you’ll type to add this emoji to messages and
              reactions.
            </p>
          </div>
          <div className="relative">
            <span className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground">
              :
            </span>
            <Input
              id="custom-emoji-name"
              data-testid="custom-emoji-name-input"
              autoCapitalize="none"
              autoCorrect="off"
              className="px-6"
              placeholder="party-parrot"
              spellCheck={false}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
            <span className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground">
              :
            </span>
          </div>
          {nameInvalid ? (
            <p className="text-sm text-destructive">
              Use only letters, numbers, hyphen, or underscore.
            </p>
          ) : pendingUpload === null ? (
            <p className="text-sm text-muted-foreground">
              Choose an image first; Sprout will suggest a name from the
              filename.
            </p>
          ) : ownDuplicate ? (
            <p className="text-sm text-muted-foreground">
              You already have :{normalized}: — saving will replace its image.
            </p>
          ) : null}
        </div>

        <div className="flex justify-end gap-2 border-t pt-4">
          <Button
            type="button"
            variant="outline"
            onClick={handleReset}
            disabled={
              setEmoji.isPending || (name.length === 0 && !pendingUpload)
            }
          >
            Clear
          </Button>
          <Button
            type="submit"
            data-testid="custom-emoji-add"
            disabled={!canSubmit}
          >
            {setEmoji.isPending ? "Saving…" : "Save emoji"}
          </Button>
        </div>
      </form>

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
