import { getCurrentWindow } from "@tauri-apps/api/window";
import { useLayoutEffect } from "react";

import { AppShell } from "@/app/AppShell";

export function App() {
  useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  return <AppShell />;
}
