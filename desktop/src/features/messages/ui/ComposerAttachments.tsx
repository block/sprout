import * as React from "react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { X } from "lucide-react";

import type { BlobDescriptor } from "@/shared/api/tauri";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { shortHash } from "@/features/messages/lib/useMediaUpload";

type ComposerAttachmentsProps = {
  attachments: BlobDescriptor[];
  onRemove: (url: string) => void;
};

/**
 * Thumbnail previews for uploaded attachments in the composer.
 * Each attachment shows as a small image with a remove button and
 * a short hash label (e.g. "a3f2").
 */
export const ComposerAttachments = React.memo(function ComposerAttachments({
  attachments,
  onRemove,
}: ComposerAttachmentsProps) {
  if (attachments.length === 0) return null;

  return (
    <LayoutGroup>
      <motion.div
        layout
        className="flex items-center gap-2"
        transition={{ type: "spring", stiffness: 500, damping: 30 }}
      >
        <AnimatePresence mode="popLayout">
          {attachments.map((attachment) => {
            const hash = shortHash(attachment.sha256);
            const isVideo = attachment.type.startsWith("video/");
            const thumbUrl = attachment.thumb
              ? rewriteRelayUrl(attachment.thumb)
              : rewriteRelayUrl(attachment.url);

            return (
              <motion.div
                key={attachment.url}
                layout
                initial={{ opacity: 0, scale: 0.8 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.8 }}
                transition={{ type: "spring", stiffness: 500, damping: 30 }}
                className="group relative"
              >
                <div className="relative h-5 w-10 overflow-hidden rounded border border-border/70">
                  {isVideo ? (
                    <div className="flex h-full w-full items-center justify-center bg-muted text-[10px] text-muted-foreground">
                      ▶
                    </div>
                  ) : (
                    <img
                      src={thumbUrl}
                      alt={`Attachment ${hash}`}
                      className="h-full w-full object-contain"
                    />
                  )}
                  <button
                    type="button"
                    onClick={() => onRemove(attachment.url)}
                    className="absolute -right-1 -top-1 hidden h-4 w-4 items-center justify-center rounded-full bg-foreground text-background group-hover:flex"
                    title="Remove attachment"
                  >
                    <X className="h-2.5 w-2.5" />
                  </button>
                </div>
              </motion.div>
            );
          })}
        </AnimatePresence>
      </motion.div>
    </LayoutGroup>
  );
});
