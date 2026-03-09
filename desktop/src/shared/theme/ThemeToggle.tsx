import { MonitorCog, Moon, Sun } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { useTheme } from "@/shared/theme/ThemeProvider";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

const themeOptions = [
  {
    value: "light",
    label: "Light",
    icon: Sun,
  },
  {
    value: "dark",
    label: "Dark",
    icon: Moon,
  },
  {
    value: "system",
    label: "System",
    icon: MonitorCog,
  },
] as const;

function getThemeLabel(theme: "light" | "dark" | "system") {
  if (theme === "light") {
    return "Light";
  }

  if (theme === "dark") {
    return "Dark";
  }

  return "System";
}

function getThemeIcon(theme: "light" | "dark" | "system") {
  if (theme === "light") {
    return Sun;
  }

  if (theme === "dark") {
    return Moon;
  }

  return MonitorCog;
}

type ThemeToggleProps = {
  className?: string;
};

export function ThemeToggle({ className }: ThemeToggleProps) {
  const { resolvedTheme, setTheme, theme } = useTheme();
  const ActiveIcon = getThemeIcon(theme);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label={`Select theme. Current setting is ${theme}${theme === "system" ? `, resolved to ${resolvedTheme}` : ""}.`}
          className={cn(
            "h-8 w-8 rounded-lg text-muted-foreground hover:text-foreground",
            className,
          )}
          size="icon"
          title="Theme"
          type="button"
          variant="ghost"
        >
          <span className="sr-only">{getThemeLabel(theme)}</span>
          <ActiveIcon className="h-4 w-4 shrink-0" />
        </Button>
      </DropdownMenuTrigger>

      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup
          onValueChange={(value) =>
            setTheme(value as "light" | "dark" | "system")
          }
          value={theme}
        >
          {themeOptions.map(({ value, label, icon: Icon }) => (
            <DropdownMenuRadioItem className="gap-2" key={value} value={value}>
              <Icon className="h-4 w-4" />
              <span>{label}</span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
