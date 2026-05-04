import { Outlet, createRootRoute } from "@tanstack/react-router";
import { ThemeToggle } from "@/shared/theme/ThemeToggle";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="flex h-12 items-center justify-between border-b px-4">
        <span className="text-sm font-semibold tracking-tight">Sprout</span>
        <ThemeToggle />
      </header>
      <main className="flex flex-1 flex-col">
        <Outlet />
      </main>
    </div>
  );
}
