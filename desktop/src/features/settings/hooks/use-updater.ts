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

export function useUpdater() {
  const [status, setStatus] = useState<UpdateStatus>({ state: "idle" });
  const updateRef = useRef<Update | null>(null);

  const closeUpdate = useCallback(async () => {
    if (updateRef.current) {
      await updateRef.current.close();
      updateRef.current = null;
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
          id: "update-available",
          description: `Version ${update.version} is ready to download.`,
        });
      } else {
        setStatus({ state: "up-to-date" });
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (
        msg.includes("plugin updater not found") ||
        msg.includes("not initialized")
      ) {
        setStatus({ state: "idle" });
        return;
      }
      setStatus({ state: "error", message: msg });
    }
  }, [closeUpdate]);

  const downloadAndInstall = useCallback(async () => {
    try {
      const update = updateRef.current;
      if (!update) {
        setStatus({ state: "up-to-date" });
        return;
      }

      setStatus({ state: "downloading" });

      await update.downloadAndInstall((event) => {
        if (event.event === "Finished") {
          setStatus({ state: "installing" });
        }
      });

      setStatus({ state: "ready" });
    } catch (err) {
      setStatus({
        state: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }, []);

  const handleRelaunch = useCallback(async () => {
    try {
      await relaunch();
    } catch (err) {
      setStatus({
        state: "error",
        message: err instanceof Error ? err.message : String(err),
      });
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
