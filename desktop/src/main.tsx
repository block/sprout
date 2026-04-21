import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { App } from "@/app/App";
import "@/shared/styles/globals.css";
import { ThemeProvider } from "@/shared/theme/ThemeProvider";
import { TooltipProvider } from "@/shared/ui/tooltip";

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
        <ThemeProvider defaultTheme="houston">
          <TooltipProvider delayDuration={300}>
            <App />
          </TooltipProvider>
        </ThemeProvider>
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
