import { useState, useRef, useCallback, useEffect } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";

export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "up-to-date" }
  | { state: "available"; version: string }
  | { state: "downloading" }
  | { state: "installing" }
  | { state: "ready" }
  | { state: "error"; message: string };

const TOAST_ID = "update-available";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function isUpdaterUnavailable(message: string): boolean {
  return (
    message.includes("plugin updater not found") ||
    message.includes("not initialized")
  );
}

export function useUpdater() {
  const [status, setStatus] = useState<UpdateStatus>({ state: "idle" });
  const updateRef = useRef<Update | null>(null);

  const closeUpdate = useCallback(async () => {
    const current = updateRef.current;
    if (current) {
      updateRef.current = null;
      await current.close();
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    try {
      const update = updateRef.current;
      if (!update) {
        return;
      }

      toast.dismiss(TOAST_ID);
      setStatus({ state: "downloading" });

      await update.downloadAndInstall((event) => {
        if (event.event === "Finished") {
          setStatus({ state: "installing" });
        }
      });

      updateRef.current = null;
      setStatus({ state: "ready" });
    } catch (err) {
      setStatus({ state: "error", message: toErrorMessage(err) });
    }
  }, []);

  const checkForUpdate = useCallback(async () => {
    try {
      await closeUpdate();
      setStatus({ state: "checking" });
      const update = await check();

      if (update) {
        updateRef.current = update;
        setStatus({ state: "available", version: update.version });
        toast("Update Available", {
          id: TOAST_ID,
          description: `Version ${update.version} is ready to download.`,
          duration: Infinity,
          action: {
            label: "Download & install",
            onClick: () => downloadAndInstall(),
          },
        });
      } else {
        setStatus({ state: "up-to-date" });
      }
    } catch (err) {
      const message = toErrorMessage(err);
      if (isUpdaterUnavailable(message)) {
        setStatus({ state: "idle" });
        return;
      }
      setStatus({ state: "error", message });
    }
  }, [closeUpdate, downloadAndInstall]);

  const handleRelaunch = useCallback(async () => {
    try {
      await relaunch();
    } catch (err) {
      setStatus({ state: "error", message: toErrorMessage(err) });
    }
  }, []);

  useEffect(() => {
    checkForUpdate();
    return () => {
      closeUpdate();
    };
  }, [checkForUpdate, closeUpdate]);

  return {
    status,
    checkForUpdate,
    downloadAndInstall,
    relaunch: handleRelaunch,
  };
}
