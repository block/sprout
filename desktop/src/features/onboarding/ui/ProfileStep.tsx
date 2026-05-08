import * as React from "react";
import { Check, KeyRound, Loader2, Upload, UserRound } from "lucide-react";

import { AvatarUpload } from "@/features/profile/ui/AvatarUpload";
import { nsecToNpub, shortenNpub } from "@/shared/lib/nostrUtils";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import type { ProfileStepActions, ProfileStepState } from "./types";

type ProfileStepProps = {
  actions: ProfileStepActions;
  state: ProfileStepState;
};

function ErrorBanner({ message }: { message: string | null }) {
  if (!message) {
    return null;
  }

  return (
    <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
      {message}
    </p>
  );
}

/**
 * Import-key flow.
 *
 * UX goals:
 * - Treat the input as a password field (masked) so over-the-shoulder peeks
 *   don't leak the secret.
 * - Accept a `.key` (or any text) file dropped onto the section: read its
 *   contents, trim, and use as the nsec.
 * - As soon as the value parses as a valid `nsec1…`, decode it and show the
 *   matching `npub1…` inline so the user can confirm *before* committing.
 * - On success, the parent (`OnboardingFlow`) invalidates the identity
 *   query, which causes `App.tsx` to remount this whole subtree under the
 *   new pubkey — local state here resets naturally; no reload needed.
 */
function ImportKeySection({
  onImport,
}: {
  onImport: (nsec: string) => Promise<void>;
}) {
  const [expanded, setExpanded] = React.useState(false);
  const [nsecInput, setNsecInput] = React.useState("");
  const [isImporting, setIsImporting] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [isDragging, setIsDragging] = React.useState(false);

  // Live-decode the current input. `null` means "not (yet) a valid nsec";
  // we only show the preview once decoding succeeds, so partial typing
  // doesn't flicker errors at the user.
  const previewNpub = React.useMemo(() => nsecToNpub(nsecInput), [nsecInput]);
  const trimmedInput = nsecInput.trim();
  const hasInput = trimmedInput.length > 0;
  const isValid = previewNpub !== null;
  const showInvalidHint = hasInput && !isValid && trimmedInput.length >= 5;

  const handleImport = React.useCallback(async () => {
    if (!previewNpub) {
      setError("That doesn't look like a valid nsec. Paste an nsec1… key.");
      return;
    }

    setIsImporting(true);
    setError(null);

    try {
      await onImport(trimmedInput);
      // On success the parent invalidates the identity query and this
      // component remounts via App.tsx's `key={currentPubkey}`. We don't
      // need to clear local state here, but we still flip `isImporting`
      // back off in case the remount is delayed (e.g. cache settling).
      setIsImporting(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import key.");
      setIsImporting(false);
    }
  }, [onImport, previewNpub, trimmedInput]);

  const handleFiles = React.useCallback(async (files: FileList | null) => {
    const file = files?.[0];
    if (!file) {
      return;
    }
    // Cap at 1 KB to avoid accidentally reading something huge if the user
    // drops the wrong file. A bech32 nsec is ~63 chars; even with trailing
    // whitespace this is plenty.
    if (file.size > 1024) {
      setError(
        "That file is too large to be a key. Drop a .key file or paste your nsec.",
      );
      return;
    }
    try {
      const text = await file.text();
      // Take the first non-empty line — tolerates trailing newlines from
      // `echo nsec1… > identity.key` and similar.
      const firstLine =
        text.split(/\r?\n/).find((line) => line.trim().length > 0) ?? "";
      setNsecInput(firstLine.trim());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't read that file.");
    }
  }, []);

  if (!expanded) {
    return (
      <button
        className="flex w-full items-center justify-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
        onClick={() => setExpanded(true)}
        type="button"
      >
        <KeyRound className="h-3 w-3" />I already have a Nostr key
      </button>
    );
  }

  return (
    <fieldset
      className={`space-y-3 rounded-[28px] border bg-muted/20 p-5 transition-colors ${
        isDragging ? "border-primary/60 bg-primary/5" : "border-border/70"
      }`}
      onDragEnter={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragging(true);
      }}
      onDragLeave={(e) => {
        e.preventDefault();
        e.stopPropagation();
        // Only clear when leaving the section itself (not a child).
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) {
          return;
        }
        setIsDragging(false);
      }}
      onDragOver={(e) => {
        e.preventDefault();
        e.stopPropagation();
        // Required for drop to fire.
      }}
      onDrop={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragging(false);
        void handleFiles(e.dataTransfer.files);
      }}
    >
      <div className="space-y-1.5">
        <label
          className="text-xs font-medium text-muted-foreground"
          htmlFor="onboarding-nsec-import"
        >
          Private key (nsec)
        </label>
        <Input
          autoComplete="off"
          autoCorrect="off"
          data-testid="onboarding-nsec-input"
          id="onboarding-nsec-import"
          onChange={(e) => {
            setNsecInput(e.target.value);
            setError(null);
          }}
          placeholder="nsec1… (or drop a .key file)"
          spellCheck={false}
          type="password"
          value={nsecInput}
        />
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Upload className="h-3 w-3" />
          Drop a `.key` file anywhere in this box, or paste your nsec.
        </p>
      </div>

      {/* Live preview of the resolved npub once the input is valid. */}
      {isValid && previewNpub ? (
        <div
          className="flex items-start gap-2 rounded-2xl border border-primary/30 bg-primary/5 px-3 py-2 text-xs"
          data-testid="onboarding-nsec-preview"
        >
          <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
          <div className="min-w-0 space-y-0.5">
            <p className="font-medium text-foreground">
              This will switch your identity to:
            </p>
            <p className="break-all font-mono text-[11px] text-muted-foreground">
              {shortenNpub(previewNpub)}
            </p>
          </div>
        </div>
      ) : null}

      {showInvalidHint && !error ? (
        <p className="text-xs text-muted-foreground">
          Waiting for a valid `nsec1…` key.
        </p>
      ) : null}

      {error ? <p className="text-sm text-destructive">{error}</p> : null}

      <div className="flex gap-2">
        <Button
          className="flex-1"
          disabled={!isValid || isImporting}
          onClick={() => {
            void handleImport();
          }}
          type="button"
        >
          {isImporting ? (
            <>
              <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
              Importing…
            </>
          ) : (
            "Use this key"
          )}
        </Button>
        <Button
          disabled={isImporting}
          onClick={() => {
            setExpanded(false);
            setNsecInput("");
            setError(null);
          }}
          type="button"
          variant="ghost"
        >
          Cancel
        </Button>
      </div>
    </fieldset>
  );
}

export function ProfileStep({ actions, state }: ProfileStepProps) {
  const {
    advanceWithoutSaving,
    clearAvatarDraft,
    importIdentity,
    onUploadingChange,
    skipForNow,
    submit,
    updateAvatarUrl,
    updateDisplayName,
  } = actions;
  const {
    avatar,
    currentNpub,
    isUploadingAvatar,
    isSaving,
    name,
    saveRecovery,
  } = state;
  const { draftValue: displayNameDraft, savedValue: savedDisplayName } = name;
  const canSubmit =
    displayNameDraft.trim().length > 0 && !isSaving && !isUploadingAvatar;
  const avatarPreviewLabel =
    displayNameDraft.trim() || savedDisplayName || "You";

  return (
    <div className="space-y-6" data-testid="onboarding-page-1">
      <div className="space-y-3">
        <Badge variant="info">First run</Badge>
        <div className="space-y-2">
          <h1 className="text-3xl font-semibold tracking-tight text-foreground">
            Set up your profile
          </h1>
          <p className="max-w-xl text-sm leading-6 text-muted-foreground">
            Add the name people will see in Sprout. A photo is optional, but it
            helps people spot you faster.
          </p>
          {/* Show the active identity so the user can confirm which key
              they're saving the profile for — and so it's obvious when
              they need to swap to a different key (e.g. an allowlisted
              one for a gated relay). */}
          {currentNpub ? (
            <p
              className="font-mono text-[11px] text-muted-foreground"
              data-testid="onboarding-current-npub"
            >
              You are{" "}
              <span className="text-foreground">
                {shortenNpub(currentNpub)}
              </span>
            </p>
          ) : null}
        </div>
      </div>

      <div className="space-y-2">
        <label
          className="text-sm font-medium"
          htmlFor="onboarding-display-name"
        >
          Display name
        </label>
        <div className="relative">
          <UserRound className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            className="pl-9"
            data-testid="onboarding-display-name"
            disabled={isSaving}
            id="onboarding-display-name"
            onChange={(event) => updateDisplayName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && canSubmit) {
                event.preventDefault();
                submit();
              }
            }}
            placeholder="How people should see you"
            value={displayNameDraft}
          />
        </div>
        <p className="text-xs text-muted-foreground">
          You can change this later in Profile settings.
        </p>
      </div>

      <AvatarUpload
        avatarUrl={avatar.draftUrl}
        previewName={avatarPreviewLabel}
        onUrlChange={updateAvatarUrl}
        onClear={clearAvatarDraft}
        onUploadingChange={onUploadingChange}
        showClear={
          avatar.draftUrl.length > 0 && avatar.draftUrl !== avatar.savedUrl
        }
        disabled={isSaving}
        testIdPrefix="onboarding-avatar"
      />

      <ImportKeySection onImport={importIdentity} />

      <ErrorBanner message={saveRecovery.errorMessage} />

      <div className="flex flex-wrap items-center justify-end gap-2">
        {saveRecovery.canSkipForNow ? (
          <Button
            data-testid="onboarding-skip"
            onClick={skipForNow}
            type="button"
            variant="outline"
          >
            Skip for now
          </Button>
        ) : null}
        {saveRecovery.canAdvanceWithoutSaving ? (
          <Button
            data-testid="onboarding-next-without-saving"
            onClick={advanceWithoutSaving}
            type="button"
            variant="outline"
          >
            Continue without saving
          </Button>
        ) : null}
        <Button
          data-testid="onboarding-next"
          disabled={!canSubmit}
          onClick={submit}
          type="button"
        >
          {isSaving ? "Saving..." : "Next"}
        </Button>
      </div>
    </div>
  );
}
