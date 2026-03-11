import { AtSign, Fingerprint, Link2, UserRound } from "lucide-react";
import * as React from "react";

import {
  useProfileQuery,
  useUpdateProfileMutation,
} from "@/features/profile/hooks";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Separator } from "@/shared/ui/separator";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { Textarea } from "@/shared/ui/textarea";

type ProfileSheetProps = {
  currentPubkey?: string;
  fallbackDisplayName?: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

function Section({
  title,
  description,
  children,
}: React.PropsWithChildren<{
  title: string;
  description?: string;
}>) {
  return (
    <section className="min-w-0 space-y-3">
      <div className="space-y-1">
        <h2 className="text-sm font-semibold tracking-tight">{title}</h2>
        {description ? (
          <p className="text-sm text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {children}
    </section>
  );
}

function ReadOnlyField({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId: string;
}) {
  return (
    <div className="min-w-0 space-y-1.5">
      <p className="text-sm font-medium">{label}</p>
      <div
        className="min-w-0 break-all whitespace-normal rounded-xl border border-border/80 bg-muted/25 px-3 py-2 text-sm text-muted-foreground"
        data-testid={testId}
      >
        {value}
      </div>
    </div>
  );
}

function AvatarPreview({
  avatarUrl,
  label,
}: {
  avatarUrl: string | null;
  label: string;
}) {
  const [hasError, setHasError] = React.useState(false);

  const initials = label
    .trim()
    .split(/\s+/)
    .map((part) => part[0] ?? "")
    .join("")
    .slice(0, 2)
    .toUpperCase();

  if (avatarUrl && !hasError) {
    return (
      <img
        alt={`${label} avatar`}
        className="h-16 w-16 rounded-3xl border border-border/80 object-cover shadow-sm"
        onError={() => {
          setHasError(true);
        }}
        referrerPolicy="no-referrer"
        src={avatarUrl}
      />
    );
  }

  return (
    <div className="flex h-16 w-16 items-center justify-center rounded-3xl border border-border/80 bg-primary/10 text-lg font-semibold text-primary shadow-sm">
      {initials.length > 0 ? initials : <UserRound className="h-6 w-6" />}
    </div>
  );
}

export function ProfileSheet({
  currentPubkey,
  fallbackDisplayName,
  open,
  onOpenChange,
}: ProfileSheetProps) {
  const profileQuery = useProfileQuery(open);
  const updateProfileMutation = useUpdateProfileMutation();
  const profile = profileQuery.data;

  const currentDisplayName = profile?.displayName ?? "";
  const currentAvatarUrl = profile?.avatarUrl ?? "";
  const currentAbout = profile?.about ?? "";

  const [displayNameDraft, setDisplayNameDraft] = React.useState("");
  const [avatarUrlDraft, setAvatarUrlDraft] = React.useState("");
  const [aboutDraft, setAboutDraft] = React.useState("");

  React.useEffect(() => {
    if (!open) {
      return;
    }

    setDisplayNameDraft(currentDisplayName);
    setAvatarUrlDraft(currentAvatarUrl);
    setAboutDraft(currentAbout);
  }, [currentAbout, currentAvatarUrl, currentDisplayName, open]);

  const nextDisplayName = displayNameDraft.trim();
  const nextAvatarUrl = avatarUrlDraft.trim();
  const nextAbout = aboutDraft.trim();

  const updatePayload: {
    displayName?: string;
    avatarUrl?: string;
    about?: string;
  } = {};

  if (nextDisplayName.length > 0 && nextDisplayName !== currentDisplayName) {
    updatePayload.displayName = nextDisplayName;
  }
  if (nextAvatarUrl.length > 0 && nextAvatarUrl !== currentAvatarUrl) {
    updatePayload.avatarUrl = nextAvatarUrl;
  }
  if (nextAbout.length > 0 && nextAbout !== currentAbout) {
    updatePayload.about = nextAbout;
  }

  const hasPendingClearRequest =
    (currentDisplayName.length > 0 && nextDisplayName.length === 0) ||
    (currentAvatarUrl.length > 0 && nextAvatarUrl.length === 0) ||
    (currentAbout.length > 0 && nextAbout.length === 0);
  const canSave =
    Object.keys(updatePayload).length > 0 && !updateProfileMutation.isPending;

  const resolvedName =
    nextDisplayName ||
    profile?.displayName ||
    fallbackDisplayName ||
    "Your profile";
  const resolvedPubkey = profile?.pubkey ?? currentPubkey ?? "Unavailable";
  const resolvedAvatarUrl =
    nextAvatarUrl.length > 0 ? nextAvatarUrl : (profile?.avatarUrl ?? null);
  const nip05Handle = profile?.nip05Handle ?? "Not set";

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent
        className="flex w-full min-w-0 flex-col gap-0 overflow-hidden border-l border-border/80 bg-background p-0 sm:max-w-lg"
        data-testid="profile-sheet"
        side="right"
      >
        <SheetHeader className="space-y-4 border-b border-border/80 bg-muted/20 px-6 py-6 text-left">
          <div className="flex min-w-0 items-start gap-4">
            <AvatarPreview
              avatarUrl={resolvedAvatarUrl}
              key={resolvedAvatarUrl ?? "profile-fallback-avatar"}
              label={resolvedName}
            />
            <div className="min-w-0 space-y-2">
              <SheetTitle className="break-words pr-8">
                {resolvedName}
              </SheetTitle>
              <SheetDescription>
                Manage how your identity appears across Sprout.
              </SheetDescription>
              <div className="inline-flex items-center gap-2 rounded-full border border-border/80 bg-background/70 px-3 py-1 text-xs font-medium text-muted-foreground">
                <Fingerprint className="h-3.5 w-3.5" />
                <span>Your relay profile</span>
              </div>
            </div>
          </div>
        </SheetHeader>

        <div className="min-w-0 flex-1 space-y-6 overflow-x-hidden overflow-y-auto px-6 py-6">
          {profileQuery.error instanceof Error ? (
            <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {profileQuery.error.message}
            </p>
          ) : null}

          <Section
            description="Identity comes from your keypair. These fields are read-only here."
            title="Identity"
          >
            <div className="space-y-3">
              <ReadOnlyField
                label="Public key"
                testId="profile-pubkey"
                value={resolvedPubkey}
              />
              <ReadOnlyField
                label="NIP-05 handle"
                testId="profile-nip05"
                value={nip05Handle}
              />
            </div>
          </Section>

          <Separator />

          <Section
            description="These values are stored on the relay for your current identity."
            title="Profile"
          >
            <form
              className="min-w-0 space-y-4"
              onSubmit={(event) => {
                event.preventDefault();
                if (!canSave) {
                  return;
                }

                void updateProfileMutation.mutateAsync(updatePayload);
              }}
            >
              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="profile-display-name"
                >
                  Display name
                </label>
                <div className="relative min-w-0">
                  <UserRound className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    className="pl-9"
                    data-testid="profile-display-name"
                    disabled={updateProfileMutation.isPending}
                    id="profile-display-name"
                    onChange={(event) =>
                      setDisplayNameDraft(event.target.value)
                    }
                    placeholder="How people should see you"
                    value={displayNameDraft}
                  />
                </div>
              </div>

              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="profile-avatar-url"
                >
                  Avatar URL
                </label>
                <div className="relative min-w-0">
                  <Link2 className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    className="pl-9"
                    data-testid="profile-avatar-url"
                    disabled={updateProfileMutation.isPending}
                    id="profile-avatar-url"
                    onChange={(event) => setAvatarUrlDraft(event.target.value)}
                    placeholder="https://example.com/avatar.png"
                    value={avatarUrlDraft}
                  />
                </div>
              </div>

              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="profile-about">
                  About
                </label>
                <div className="relative min-w-0">
                  <AtSign className="pointer-events-none absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
                  <Textarea
                    className="min-h-28 pl-9"
                    data-testid="profile-about"
                    disabled={updateProfileMutation.isPending}
                    id="profile-about"
                    onChange={(event) => setAboutDraft(event.target.value)}
                    placeholder="A short description for your profile"
                    value={aboutDraft}
                  />
                </div>
              </div>

              <Button
                data-testid="profile-save"
                disabled={!canSave}
                size="sm"
                type="submit"
              >
                {updateProfileMutation.isPending ? "Saving..." : "Save profile"}
              </Button>

              {hasPendingClearRequest ? (
                <p className="text-sm text-muted-foreground">
                  Clearing existing profile fields is not supported yet. Blank
                  fields are ignored for now.
                </p>
              ) : null}

              {updateProfileMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {updateProfileMutation.error.message}
                </p>
              ) : null}
            </form>
          </Section>
        </div>
      </SheetContent>
    </Sheet>
  );
}
