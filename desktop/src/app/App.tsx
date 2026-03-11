import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import { AppShell } from "@/app/AppShell";

export function App() {
  useEffect(() => {
    getCurrentWindow().show();
  }, []);

  return <AppShell />;
}
