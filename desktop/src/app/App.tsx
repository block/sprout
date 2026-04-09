import { getCurrentWindow } from "@tauri-apps/api/window";
import { RouterProvider } from "@tanstack/react-router";
import { useLayoutEffect } from "react";

import { IdentityGate } from "@/app/IdentityGate";
import { router } from "@/app/router";

export function App() {
  useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  return (
    <IdentityGate>
      <RouterProvider router={router} />
    </IdentityGate>
  );
}
