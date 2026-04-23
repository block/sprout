import { useUpdaterContext } from "./hooks/UpdaterProvider";
import { Button } from "@/shared/ui/button";
import { Badge } from "@/shared/ui/badge";

export function UpdateChecker() {
  const { status, checkForUpdate, downloadAndInstall, relaunch } =
    useUpdaterContext();

  return (
    <section className="min-w-0">
      <div className="mb-3 min-w-0">
        <h2 className="text-sm font-semibold tracking-tight">
          Software Updates
        </h2>
        <p className="text-sm text-muted-foreground">
          Keep Sprout up to date with the latest features and fixes.
        </p>
      </div>

      {status.state === "idle" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-muted-foreground">
            Check if a new version is available.
          </p>
          <Button size="sm" onClick={checkForUpdate}>
            Check for Updates
          </Button>
        </div>
      )}

      {status.state === "checking" && (
        <p className="text-sm text-muted-foreground">Checking for updates...</p>
      )}

      {status.state === "up-to-date" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-foreground">
            You're on the latest version.
          </p>
          <Button variant="outline" size="sm" onClick={checkForUpdate}>
            Check Again
          </Button>
        </div>
      )}

      {status.state === "available" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-foreground">
            Version <Badge variant="info">{status.version}</Badge> is available.
          </p>
          <Button size="sm" onClick={downloadAndInstall}>
            Download &amp; Install
          </Button>
        </div>
      )}

      {status.state === "downloading" && (
        <p className="text-sm text-muted-foreground">Downloading update...</p>
      )}

      {status.state === "installing" && (
        <p className="text-sm text-muted-foreground">Installing update...</p>
      )}

      {status.state === "ready" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-foreground">
            Update installed. Restart to apply.
          </p>
          <Button size="sm" onClick={relaunch}>
            Restart Now
          </Button>
        </div>
      )}

      {status.state === "error" && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-destructive">
            Update failed: {status.message}
          </p>
          <Button variant="outline" size="sm" onClick={checkForUpdate}>
            Retry
          </Button>
        </div>
      )}
    </section>
  );
}
