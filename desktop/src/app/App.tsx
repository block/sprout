import { getCurrentWindow } from "@tauri-apps/api/window";
import { RouterProvider } from "@tanstack/react-router";
import { useLayoutEffect } from "react";

import { router } from "@/app/router";

export function App() {
  useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  return <RouterProvider router={router} />;
}
