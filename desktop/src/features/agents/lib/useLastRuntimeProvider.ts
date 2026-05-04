import * as React from "react";

const STORAGE_KEY = "sprout:last-runtime-provider";

export function useLastRuntimeProvider(): {
  lastProviderId: string | null;
  setLastProvider: (id: string) => void;
} {
  const [lastProviderId, setLastProviderId] = React.useState<string | null>(
    () => {
      try {
        return localStorage.getItem(STORAGE_KEY);
      } catch {
        return null;
      }
    },
  );

  const setLastProvider = React.useCallback((id: string) => {
    setLastProviderId(id);
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {
      // localStorage full — ignore
    }
  }, []);

  return { lastProviderId, setLastProvider };
}
