import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { notifyManager } from "@tanstack/react-query";
import { App } from "@/app/App";
import "@/shared/styles/globals.css";
import { WorkspacesProvider } from "@/features/workspaces/useWorkspaces";
import { ThemeProvider } from "@/shared/theme/ThemeProvider";
import { Toaster } from "@/shared/ui/sonner";
import { TooltipProvider } from "@/shared/ui/tooltip";

// Override React Query's default setTimeout(cb, 0) scheduler with
// MessageChannel. WebKit throttles setTimeout to 1s boundaries when
// the window is occluded; MessageChannel macrotasks bypass this.
const notifyChannel = new MessageChannel();
const notifyQueue: Array<() => void> = [];
notifyChannel.port1.onmessage = () => {
  const fns = notifyQueue.splice(0);
  for (const fn of fns) {
    try {
      fn();
    } catch (e) {
      console.error(e);
    }
  }
};
notifyManager.setScheduler((cb) => {
  notifyQueue.push(cb);
  notifyChannel.port2.postMessage(null);
});

type E2eWindow = Window & {
  __SPROUT_E2E__?: unknown;
};

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      networkMode: "always",
      gcTime: 5 * 60 * 1_000,
    },
    mutations: {
      networkMode: "always",
    },
  },
});

function renderApp() {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <WorkspacesProvider>
          <ThemeProvider defaultTheme="houston">
            <TooltipProvider delayDuration={300}>
              <App />
              <Toaster />
            </TooltipProvider>
          </ThemeProvider>
        </WorkspacesProvider>
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

async function installE2eBridgeIfConfigured() {
  // Keep the large E2E bridge out of the normal startup path and production
  // bundle; only load it when tests explicitly inject an E2E config.
  if (!(window as E2eWindow).__SPROUT_E2E__) {
    return;
  }

  const { maybeInstallE2eTauriMocks } = await import("@/testing/e2eBridge");
  maybeInstallE2eTauriMocks();
}

async function bootstrap() {
  await installE2eBridgeIfConfigured();
  renderApp();
}

void bootstrap();
