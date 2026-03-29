import * as React from "react";

import {
  type BlobDescriptor,
  pickAndUploadMedia,
  uploadMediaBytes,
} from "@/shared/api/tauri";

const ALLOWED_TYPES = ["image/jpeg", "image/png", "image/gif", "image/webp"];

type UploadState = {
  status: "idle" | "uploading" | "error";
  message?: string;
};

export function useMediaUpload(
  setContent: React.Dispatch<React.SetStateAction<string>>,
) {
  const [uploadState, setUploadState] = React.useState<UploadState>({
    status: "idle",
  });
  const [pendingImeta, setPendingImeta] = React.useState<BlobDescriptor[]>([]);

  const pendingImetaRef = React.useRef(pendingImeta);
  pendingImetaRef.current = pendingImeta;

  const onUploaded = React.useCallback(
    (descriptor: BlobDescriptor) => {
      const markdown = `\n![image](${descriptor.url})\n`;
      setContent((prev) => prev + markdown);
      setPendingImeta((prev) => [...prev, descriptor]);
      setUploadState({ status: "idle" });
    },
    [setContent],
  );

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

      if (!ALLOWED_TYPES.includes(file.type)) {
        setUploadState({
          status: "error",
          message: "Only JPEG, PNG, GIF, and WebP images are supported",
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
    async (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const items = Array.from(event.clipboardData.items);
      const imageItem = items.find((item) => ALLOWED_TYPES.includes(item.type));
      if (!imageItem) return;

      event.preventDefault();
      const file = imageItem.getAsFile();
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

  const isUploading = uploadState.status === "uploading";

  return {
    handleDragOver,
    handleDrop,
    handlePaperclip,
    handlePaste,
    isUploading,
    pendingImeta,
    pendingImetaRef,
    setPendingImeta,
    setUploadState,
    uploadState,
  };
}
