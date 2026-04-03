import * as React from "react";

import {
  DEFAULT_SETTINGS_SECTION,
  type SettingsSection,
} from "@/features/settings/ui/SettingsView";
import type { Channel } from "@/shared/api/types";

type AppView = "home" | "channel" | "settings" | "agents";
type MainView = Exclude<AppView, "settings">;

type UseViewRouterResult = {
  selectedView: AppView;
  setSelectedView: React.Dispatch<React.SetStateAction<AppView>>;
  settingsSection: SettingsSection;
  setSettingsSection: React.Dispatch<React.SetStateAction<SettingsSection>>;
  lastNonSettingsViewRef: React.RefObject<MainView>;
  handleOpenSettings: (section?: SettingsSection) => void;
  handleCloseSettings: (selectedChannel: Channel | null) => void;
};

export function useViewRouter(
  setIsSearchOpen: (open: boolean) => void,
  setIsChannelManagementOpen: (open: boolean) => void,
): UseViewRouterResult {
  const [selectedView, setSelectedView] = React.useState<AppView>("home");
  const [settingsSection, setSettingsSection] = React.useState<SettingsSection>(
    DEFAULT_SETTINGS_SECTION,
  );
  const lastNonSettingsViewRef = React.useRef<MainView>("home");

  React.useEffect(() => {
    if (selectedView === "settings") {
      return;
    }
    lastNonSettingsViewRef.current = selectedView;
  }, [selectedView]);

  const handleOpenSettings = React.useCallback(
    (section: SettingsSection = DEFAULT_SETTINGS_SECTION) => {
      setIsSearchOpen(false);
      setIsChannelManagementOpen(false);
      setSettingsSection(section);
      React.startTransition(() => {
        setSelectedView("settings");
      });
    },
    [setIsSearchOpen, setIsChannelManagementOpen],
  );

  const handleCloseSettings = React.useCallback(
    (selectedChannel: Channel | null) => {
      const nextView: MainView =
        lastNonSettingsViewRef.current === "channel" && !selectedChannel
          ? "home"
          : lastNonSettingsViewRef.current;

      React.startTransition(() => {
        setSelectedView(nextView);
      });
    },
    [],
  );

  return {
    selectedView,
    setSelectedView,
    settingsSection,
    setSettingsSection,
    lastNonSettingsViewRef,
    handleOpenSettings,
    handleCloseSettings,
  };
}
