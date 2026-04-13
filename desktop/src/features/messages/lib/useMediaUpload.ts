import * as React from "react";

import {
  type BlobDescriptor,
  pickAndUploadMedia,
  uploadMediaBytes,
} from "@/shared/api/tauri";

export const ALLOWED_MEDIA_TYPES = [
  "image/jpeg",
  "image/png",
  "image/gif",
  "image/webp",
  "video/mp4",
  "video/quicktime",
  "video/x-matroska",
  "video/webm",
  "video/x-msvideo",
];

/**
 * First 4 hex chars of the sha256 — used as a short display name.
 * Note: 4 hex chars = 65,536 possible values. Collision is unlikely
 * within a single message's attachments but theoretically possible.
 * If collisions become an issue, extend to 6+ chars.
 */
export function shortHash(sha256: string): string {
  return sha256.slice(0, 4);
}

type UploadState = {
  status: "idle" | "uploading" | "error";
  message?: string;
};

export function useMediaUpload() {
  const [uploadState, setUploadState] = React.useState<UploadState>({
    status: "idle",
  });
  const [pendingImeta, setPendingImeta] = React.useState<BlobDescriptor[]>([]);

  const pendingImetaRef = React.useRef(pendingImeta);
  pendingImetaRef.current = pendingImeta;

  const onUploaded = React.useCallback((descriptor: BlobDescriptor) => {
    setPendingImeta((prev) => [...prev, descriptor]);
    setUploadState({ status: "idle" });
  }, []);

  const handlePaperclip = React.useCallback(async () => {
    setUploadState({ status: "uploading" });
    try {
      const descriptor = await pickAndUploadMedia();
      if (descriptor) {
        onUploaded(descriptor);
      } else {
        setUploadState({ status: "idle" });
      }
    } catch (err) {
      setUploadState({ status: "error", message: String(err) });
    }
  }, [onUploaded]);

  const handleDrop = React.useCallback(
    async (event: React.DragEvent<HTMLFormElement>) => {
      event.preventDefault();
      const files = Array.from(event.dataTransfer.files);
      if (files.length === 0) return;

      const file = files[0];
      if (!file) return;

      if (!ALLOWED_MEDIA_TYPES.includes(file.type)) {
        setUploadState({
          status: "error",
          message:
            "Unsupported file type. Supported: JPEG, PNG, GIF, WebP, MP4, MOV, MKV, WebM, AVI",
        });
        return;
      }

      setUploadState({ status: "uploading" });
      try {
        const buffer = await file.arrayBuffer();
        const descriptor = await uploadMediaBytes([...new Uint8Array(buffer)]);
        onUploaded(descriptor);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [onUploaded],
  );

  const handleDragOver = React.useCallback(
    (event: React.DragEvent<HTMLFormElement>) => {
      event.preventDefault();
    },
    [],
  );

  const handlePaste = React.useCallback(
    async (event: {
      clipboardData: DataTransfer;
      preventDefault: () => void;
    }) => {
      const items = Array.from(event.clipboardData.items);
      const mediaItem = items.find((item) =>
        ALLOWED_MEDIA_TYPES.includes(item.type),
      );
      if (!mediaItem) return;

      event.preventDefault();
      const file = mediaItem.getAsFile();
      if (!file) return;

      setUploadState({ status: "uploading" });
      try {
        const buffer = await file.arrayBuffer();
        const descriptor = await uploadMediaBytes([...new Uint8Array(buffer)]);
        onUploaded(descriptor);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [onUploaded],
  );

  /** Upload a File directly — used by Tiptap's editorProps.handlePaste. */
  const uploadFile = React.useCallback(
    async (file: File) => {
      if (!ALLOWED_MEDIA_TYPES.includes(file.type)) return;
      setUploadState({ status: "uploading" });
      try {
        const buffer = await file.arrayBuffer();
        const descriptor = await uploadMediaBytes([...new Uint8Array(buffer)]);
        onUploaded(descriptor);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [onUploaded],
  );

  const removeAttachment = React.useCallback((url: string) => {
    setPendingImeta((prev) => prev.filter((d) => d.url !== url));
  }, []);

  const isUploading = uploadState.status === "uploading";

  return {
    handleDragOver,
    handleDrop,
    handlePaperclip,
    handlePaste,
    isUploading,
    pendingImeta,
    pendingImetaRef,
    removeAttachment,
    setPendingImeta,
    setUploadState,
    uploadFile,
    uploadState,
  };
}
