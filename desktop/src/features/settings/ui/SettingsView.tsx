import {
  AtSign,
  Check,
  Fingerprint,
  Link2,
  MonitorCog,
  Moon,
  Sun,
  UserRound,
  type LucideIcon,
} from "lucide-react";
import * as React from "react";

import {
  useProfileQuery,
  useUpdateProfileMutation,
} from "@/features/profile/hooks";
import { cn } from "@/shared/lib/cn";
import { useTheme } from "@/shared/theme/ThemeProvider";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Separator } from "@/shared/ui/separator";
import { Textarea } from "@/shared/ui/textarea";

type SettingsViewProps = {
  currentPubkey?: string;
  fallbackDisplayName?: string;
};

type ThemeOption = {
  value: "light" | "dark" | "system";
  label: string;
  icon: LucideIcon;
};

const themeOptions: ThemeOption[] = [
  {
    value: "light",
    label: "Light",
    icon: Sun,
  },
  {
    value: "dark",
    label: "Dark",
    icon: Moon,
  },
  {
    value: "system",
    label: "System",
    icon: MonitorCog,
  },
];

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

function ThemeSettingsCard() {
  const { setTheme, theme } = useTheme();

  return (
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-4 shadow-sm"
      data-testid="settings-theme"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold tracking-tight">Appearance</h2>
          <p className="text-sm text-muted-foreground">
            Choose how Sprout looks on this device.
          </p>
        </div>

        <div className="inline-flex w-full flex-col gap-1 rounded-xl border border-border/70 bg-background/70 p-1 sm:w-auto sm:flex-row">
          {themeOptions.map(({ value, label, icon: Icon }) => {
            const isActive = theme === value;

            return (
              <button
                aria-pressed={isActive}
                className={cn(
                  "inline-flex items-center justify-center gap-2 rounded-lg px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  isActive
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
                data-testid={`theme-option-${value}`}
                key={value}
                onClick={() => {
                  setTheme(value);
                }}
                type="button"
              >
                <Icon className="h-4 w-4" />
                <span>{label}</span>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}

function ProfileSettingsCard({
  currentPubkey,
  fallbackDisplayName,
}: SettingsViewProps) {
  const profileQuery = useProfileQuery();
  const updateProfileMutation = useUpdateProfileMutation();
  const profile = profileQuery.data;

  const currentDisplayName = profile?.displayName ?? "";
  const currentAvatarUrl = profile?.avatarUrl ?? "";
  const currentAbout = profile?.about ?? "";

  const [displayNameDraft, setDisplayNameDraft] = React.useState("");
  const [avatarUrlDraft, setAvatarUrlDraft] = React.useState("");
  const [aboutDraft, setAboutDraft] = React.useState("");

  React.useEffect(() => {
    setDisplayNameDraft(currentDisplayName);
    setAvatarUrlDraft(currentAvatarUrl);
    setAboutDraft(currentAbout);
  }, [currentAbout, currentAvatarUrl, currentDisplayName]);

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
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-5 shadow-sm"
      data-testid="settings-profile"
    >
      <div className="flex min-w-0 items-start gap-4">
        <AvatarPreview
          avatarUrl={resolvedAvatarUrl}
          key={resolvedAvatarUrl ?? "profile-fallback-avatar"}
          label={resolvedName}
        />
        <div className="min-w-0 space-y-2">
          <div>
            <h2 className="break-words text-base font-semibold tracking-tight">
              {resolvedName}
            </h2>
            <p className="text-sm text-muted-foreground">
              Manage how your identity appears across Sprout.
            </p>
          </div>
          <div className="inline-flex items-center gap-2 rounded-full border border-border/80 bg-background/70 px-3 py-1 text-xs font-medium text-muted-foreground">
            <Fingerprint className="h-3.5 w-3.5" />
            <span>Your relay profile</span>
          </div>
        </div>
      </div>

      <div className="mt-6 space-y-6">
        {profileQuery.error instanceof Error ? (
          <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {profileQuery.error.message}
          </p>
        ) : null}

        {updateProfileMutation.isSuccess ? (
          <div className="flex items-center gap-2 rounded-xl border border-primary/20 bg-primary/10 px-3 py-2 text-sm text-primary">
            <Check className="h-4 w-4" />
            <span>Profile saved.</span>
          </div>
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
                  onChange={(event) => setDisplayNameDraft(event.target.value)}
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
          </form>
        </Section>
      </div>
    </section>
  );
}

export function SettingsView({
  currentPubkey,
  fallbackDisplayName,
}: SettingsViewProps) {
  return (
    <div
      className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6"
      data-testid="settings-view"
    >
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
        <ThemeSettingsCard />
        <ProfileSettingsCard
          currentPubkey={currentPubkey}
          fallbackDisplayName={fallbackDisplayName}
        />
      </div>
    </div>
  );
}
