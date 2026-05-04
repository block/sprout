import { Link, Outlet, createRootRoute } from "@tanstack/react-router";
import { ThemeToggle } from "@/shared/theme/ThemeToggle";

export const Route = createRootRoute({
  component: RootLayout,
});

function NavLink({ to, children }: { to: string; children: React.ReactNode }) {
  return (
    <Link
      to={to}
      className="text-sm text-muted-foreground transition-colors hover:text-foreground [&.active]:font-medium [&.active]:text-foreground"
    >
      {children}
    </Link>
  );
}

function RootLayout() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="flex h-12 items-center justify-between border-b px-4">
        <nav className="flex items-center gap-4">
          <Link
            to="/"
            className="text-sm font-semibold tracking-tight text-foreground"
          >
            Sprout
          </Link>
          <NavLink to="/repos">Repos</NavLink>
        </nav>
        <ThemeToggle />
      </header>
      <main className="flex flex-1 flex-col">
        <Outlet />
      </main>
    </div>
  );
}
