import { useState, useRef, useCallback } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "up-to-date" }
  | { state: "available"; version: string }
  | { state: "downloading" }
  | { state: "installing" }
  | { state: "ready" }
  | { state: "error"; message: string };

export function UpdateChecker() {
  const [status, setStatus] = useState<UpdateStatus>({ state: "idle" });
  const updateRef = useRef<Update | null>(null);

  const closeUpdate = useCallback(async () => {
    if (updateRef.current) {
      await updateRef.current.close();
      updateRef.current = null;
    }
  }, []);

  async function checkForUpdate() {
    try {
      await closeUpdate();
      setStatus({ state: "checking" });
      const update = await check();

      if (update) {
        updateRef.current = update;
        setStatus({ state: "available", version: update.version });
      } else {
        setStatus({ state: "up-to-date" });
      }
    } catch (err) {
      setStatus({
        state: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function downloadAndInstall() {
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
  }

  async function handleRelaunch() {
    await relaunch();
  }

  return (
    <div className="rounded-lg bg-zinc-900 p-4">
      <h3 className="mb-3 text-sm font-medium text-zinc-200">
        Software Updates
      </h3>

      {status.state === "idle" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-zinc-400">
            Check if a new version is available.
          </p>
          <button
            type="button"
            onClick={checkForUpdate}
            className="rounded-lg bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
          >
            Check for Updates
          </button>
        </div>
      )}

      {status.state === "checking" && (
        <p className="text-sm text-zinc-400">Checking for updates...</p>
      )}

      {status.state === "up-to-date" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-zinc-300">You're on the latest version.</p>
          <button
            type="button"
            onClick={checkForUpdate}
            className="rounded-lg bg-zinc-800 px-3 py-1.5 text-sm font-medium text-zinc-300 hover:bg-zinc-700"
          >
            Check Again
          </button>
        </div>
      )}

      {status.state === "available" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-zinc-300">
            Version <span className="font-semibold">{status.version}</span> is
            available.
          </p>
          <button
            type="button"
            onClick={downloadAndInstall}
            className="rounded-lg bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
          >
            Download &amp; Install
          </button>
        </div>
      )}

      {status.state === "downloading" && (
        <p className="text-sm text-zinc-400">Downloading update...</p>
      )}

      {status.state === "installing" && (
        <p className="text-sm text-zinc-400">Installing update...</p>
      )}

      {status.state === "ready" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-zinc-300">
            Update installed. Restart to apply.
          </p>
          <button
            type="button"
            onClick={handleRelaunch}
            className="rounded-lg bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
          >
            Restart Now
          </button>
        </div>
      )}

      {status.state === "error" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-red-400">
            Update failed: {status.message}
          </p>
          <button
            type="button"
            onClick={checkForUpdate}
            className="rounded-lg bg-zinc-800 px-3 py-1.5 text-sm font-medium text-zinc-300 hover:bg-zinc-700"
          >
            Retry
          </button>
        </div>
      )}
    </div>
  );
}
