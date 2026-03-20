import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { App } from "@/app/App";
import "@/shared/styles/globals.css";
import { ThemeProvider } from "@/shared/theme/ThemeProvider";
import { maybeInstallE2eTauriMocks } from "@/testing/e2eBridge";

maybeInstallE2eTauriMocks();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      networkMode: "always",
    },
    mutations: {
      networkMode: "always",
    },
  },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider defaultTheme="system">
        <App />
      </ThemeProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
