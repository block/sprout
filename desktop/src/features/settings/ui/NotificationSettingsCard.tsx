import type {
  DesktopNotificationPermissionState,
  NotificationSettings,
} from "@/features/notifications/hooks";
import {
  RECOMMENDED_SINGLE_SOUND,
  RECOMMENDED_SOUND_BY_SLOT,
  SLOT_DESCRIPTIONS,
  SLOT_LABELS,
  SOUND_SLOTS,
  type SoundMode,
  type SoundName,
  type SoundSlot,
} from "@/features/notifications/lib/sound";
import { cn } from "@/shared/lib/cn";
import { Switch } from "@/shared/ui/switch";
import { SettingsOptionGroup, SettingsOptionRow } from "./SettingsOptionGroup";
import { SoundPicker } from "./SoundPicker";

export function NotificationSettingsCard({
  isUpdatingDesktopNotifications,
  notificationErrorMessage,
  notificationPermission,
  notificationSettings,
  onSetDesktopNotificationsEnabled,
  onSetHomeBadgeEnabled,
  onSetJobProgressSoundEnabled,
  onSetMentionNotificationsEnabled,
  onSetNeedsActionNotificationsEnabled,
  onSetNotifyWhileViewing,
  onSetSingleSound,
  onSetSoundEnabled,
  onSetSoundForSlot,
  onSetSoundMode,
}: {
  isUpdatingDesktopNotifications: boolean;
  notificationErrorMessage: string | null;
  notificationPermission: DesktopNotificationPermissionState;
  notificationSettings: NotificationSettings;
  onSetDesktopNotificationsEnabled: (enabled: boolean) => Promise<boolean>;
  onSetHomeBadgeEnabled: (enabled: boolean) => void;
  onSetJobProgressSoundEnabled: (enabled: boolean) => void;
  onSetMentionNotificationsEnabled: (enabled: boolean) => void;
  onSetNeedsActionNotificationsEnabled: (enabled: boolean) => void;
  onSetNotifyWhileViewing: (enabled: boolean) => void;
  onSetSingleSound: (name: SoundName) => void;
  onSetSoundEnabled: (enabled: boolean) => void;
  onSetSoundForSlot: (slot: SoundSlot, name: SoundName | null) => void;
  onSetSoundMode: (mode: SoundMode) => void;
}) {
  const permissionBlocked =
    notificationPermission === "denied" ||
    notificationPermission === "unsupported";
  const soundControlsVisible =
    notificationSettings.desktopEnabled && notificationSettings.soundEnabled;
  const customSounds = notificationSettings.soundMode === "custom";

  return (
    <section className="min-w-0" data-testid="settings-notifications">
      <div className="mb-12 min-w-0">
        <h2 className="text-2xl font-semibold tracking-tight">Notifications</h2>
        <p className="text-base font-normal text-muted-foreground">
          Desktop alerts are on by default. Fine-tune what gets through below.
        </p>
      </div>

      <span className="sr-only" data-testid="notifications-desktop-state">
        {notificationPermission === "unsupported"
          ? "Unavailable"
          : notificationPermission === "denied"
            ? "Blocked"
            : notificationSettings.desktopEnabled
              ? "On"
              : "Off"}
      </span>

      <div className="flex flex-col gap-4">
        <SettingsOptionGroup>
          <SettingsOptionRow>
            <div className="min-w-0">
              <label
                className="text-sm font-medium"
                htmlFor="desktop-alerts-switch"
              >
                {isUpdatingDesktopNotifications
                  ? "Requesting..."
                  : "Desktop alerts"}
              </label>
              <p className="text-sm font-normal text-muted-foreground">
                {notificationSettings.desktopEnabled
                  ? "Native desktop alerts are enabled for the categories you have armed below."
                  : "Request OS permission and surface new mentions or needs-action items outside the app."}
              </p>
            </div>
            <Switch
              checked={notificationSettings.desktopEnabled}
              data-testid="notifications-desktop-toggle"
              disabled={isUpdatingDesktopNotifications}
              id="desktop-alerts-switch"
              onCheckedChange={(checked) => {
                void onSetDesktopNotificationsEnabled(checked);
              }}
            />
          </SettingsOptionRow>

          <SettingsOptionRow>
            <div className="min-w-0">
              <label
                className="text-sm font-medium"
                htmlFor="notify-while-viewing-switch"
              >
                Notify while viewing
              </label>
              <p className="text-sm font-normal text-muted-foreground">
                Also alert for direct messages in the conversation you have
                open.
              </p>
            </div>
            <Switch
              checked={notificationSettings.notifyWhileViewing}
              data-testid="notifications-notify-while-viewing-toggle"
              id="notify-while-viewing-switch"
              onCheckedChange={(checked) => {
                onSetNotifyWhileViewing(checked);
              }}
            />
          </SettingsOptionRow>

          <SettingsOptionRow>
            <div className="min-w-0">
              <label className="text-sm font-medium" htmlFor="mentions-switch">
                @Mentions
              </label>
              <p className="text-sm font-normal text-muted-foreground">
                Alert when someone tags your pubkey in a channel you can access.
              </p>
            </div>
            <Switch
              checked={notificationSettings.mentions}
              data-testid="notifications-mentions-toggle"
              id="mentions-switch"
              onCheckedChange={(checked) => {
                onSetMentionNotificationsEnabled(checked);
              }}
            />
          </SettingsOptionRow>

          <SettingsOptionRow>
            <div className="min-w-0">
              <label
                className="text-sm font-medium"
                htmlFor="needs-action-switch"
              >
                Needs action
              </label>
              <p className="text-sm font-normal text-muted-foreground">
                Alert for reminders and workflow approvals that are waiting on
                you.
              </p>
            </div>
            <Switch
              checked={notificationSettings.needsAction}
              data-testid="notifications-needs-action-toggle"
              id="needs-action-switch"
              onCheckedChange={(checked) => {
                onSetNeedsActionNotificationsEnabled(checked);
              }}
            />
          </SettingsOptionRow>
        </SettingsOptionGroup>

        <SettingsOptionGroup>
          <SettingsOptionRow>
            <div className="min-w-0">
              <label
                className="text-sm font-medium"
                htmlFor="notification-sound-switch"
              >
                Notification sound
              </label>
              <p className="text-sm font-normal text-muted-foreground">
                Play a sound when a desktop notification fires.
              </p>
            </div>
            <Switch
              checked={
                notificationSettings.desktopEnabled &&
                notificationSettings.soundEnabled
              }
              data-testid="notifications-sound-toggle"
              disabled={!notificationSettings.desktopEnabled}
              id="notification-sound-switch"
              onCheckedChange={(checked) => {
                onSetSoundEnabled(checked);
              }}
            />
          </SettingsOptionRow>

          {soundControlsVisible ? (
            <SettingsOptionRow>
              <div className="min-w-0">
                <span className="text-sm font-medium">Sound</span>
                <p className="text-sm font-normal text-muted-foreground">
                  {customSounds
                    ? "The default for events without their own sound."
                    : "Played for every notification."}
                </p>
              </div>
              <span
                className={cn(
                  "transition-opacity duration-200",
                  customSounds && "opacity-35 hover:opacity-100",
                )}
              >
                <SoundPicker
                  onChange={(next) =>
                    onSetSingleSound(next ?? RECOMMENDED_SINGLE_SOUND)
                  }
                  recommended={RECOMMENDED_SINGLE_SOUND}
                  value={notificationSettings.singleSound}
                />
              </span>
            </SettingsOptionRow>
          ) : null}

          {soundControlsVisible ? (
            <SettingsOptionRow>
              <div className="min-w-0">
                <label
                  className="text-sm font-medium"
                  htmlFor="custom-sounds-switch"
                >
                  Customize per event
                </label>
                <p className="text-sm font-normal text-muted-foreground">
                  Pick a different sound for each kind of notification.
                </p>
              </div>
              <Switch
                checked={customSounds}
                data-testid="notifications-custom-sounds-toggle"
                id="custom-sounds-switch"
                onCheckedChange={(checked) => {
                  onSetSoundMode(checked ? "custom" : "single");
                }}
              />
            </SettingsOptionRow>
          ) : null}

          {soundControlsVisible ? (
            <div
              className={
                customSounds
                  ? "grid grid-rows-[1fr] transition-[grid-template-rows] duration-200"
                  : "grid grid-rows-[0fr] transition-[grid-template-rows] duration-200"
              }
            >
              <div className="min-h-0 overflow-hidden">
                <div className="mx-4 mb-4 rounded-xl bg-muted/30">
                  {SOUND_SLOTS.map((slot) => {
                    const isJobProgress = slot === "job_progress";
                    const slotDisabled =
                      isJobProgress &&
                      !notificationSettings.jobProgressSoundEnabled;
                    return (
                      <div
                        className="flex items-center justify-between gap-4 px-4 py-2.5"
                        key={slot}
                      >
                        <div className="min-w-0">
                          <span className="text-sm font-medium">
                            {SLOT_LABELS[slot]}
                          </span>
                          <p className="text-xs font-normal text-muted-foreground">
                            {SLOT_DESCRIPTIONS[slot]}
                          </p>
                          {isJobProgress ? (
                            <div className="mt-1.5 flex items-center gap-2 text-xs text-muted-foreground">
                              <Switch
                                checked={
                                  notificationSettings.jobProgressSoundEnabled
                                }
                                id="job-progress-sound-switch"
                                onCheckedChange={(checked) => {
                                  onSetJobProgressSoundEnabled(checked);
                                }}
                              />
                              <label htmlFor="job-progress-sound-switch">
                                Play sound on every progress update
                              </label>
                            </div>
                          ) : null}
                        </div>
                        <SoundPicker
                          disabled={slotDisabled}
                          inheritFrom={notificationSettings.singleSound}
                          onChange={(next) => onSetSoundForSlot(slot, next)}
                          recommended={RECOMMENDED_SOUND_BY_SLOT[slot]}
                          value={notificationSettings.sounds[slot]}
                        />
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : null}
        </SettingsOptionGroup>

        <SettingsOptionGroup>
          <SettingsOptionRow>
            <div className="min-w-0">
              <label
                className="text-sm font-medium"
                htmlFor="home-badge-switch"
              >
                Home badge
              </label>
              <p className="text-sm font-normal text-muted-foreground">
                Show a Home badge for mentions and needs-action items in the
                sidebar.
              </p>
            </div>
            <Switch
              checked={notificationSettings.homeBadgeEnabled}
              data-testid="notifications-home-badge-toggle"
              id="home-badge-switch"
              onCheckedChange={(checked) => {
                onSetHomeBadgeEnabled(checked);
              }}
            />
          </SettingsOptionRow>
        </SettingsOptionGroup>
      </div>

      {permissionBlocked && (
        <p className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {notificationPermission === "unsupported"
            ? "Desktop notifications are not supported in this environment."
            : "Desktop notifications are blocked. Enable them in your system settings."}
        </p>
      )}

      {notificationErrorMessage ? (
        <p className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {notificationErrorMessage}
        </p>
      ) : null}
    </section>
  );
}
