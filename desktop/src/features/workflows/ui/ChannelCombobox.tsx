import { Check, ChevronsUpDown, Search } from "lucide-react";
import * as React from "react";

import type { Channel } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

type ChannelComboboxProps = {
  channels: Channel[];
  disabled?: boolean;
  onChange: (value: string) => void;
  value: string;
};

export function ChannelCombobox({
  channels,
  disabled,
  onChange,
  value,
}: ChannelComboboxProps) {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");

  const selected = channels.find((c) => c.id === value);

  const filtered = React.useMemo(() => {
    if (!query) return channels;
    const q = query.toLowerCase();
    return channels.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.channelType?.toLowerCase().includes(q) ||
        c.id.toLowerCase().includes(q),
    );
  }, [channels, query]);

  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverTrigger asChild>
        <button
          aria-expanded={open}
          className={cn(
            "flex h-9 w-full items-center justify-between rounded-md border border-input bg-transparent px-3 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
            !selected && "text-muted-foreground",
          )}
          disabled={disabled}
          role="combobox"
          type="button"
        >
          <span className="truncate">
            {selected
              ? `${selected.name} · ${selected.channelType} · ${selected.visibility}`
              : "Select a channel..."}
          </span>
          <ChevronsUpDown className="ml-2 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-[--radix-popover-trigger-width] p-0"
      >
        <div className="flex items-center gap-2 border-b border-border px-3 py-2">
          <Search className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <input
            autoCapitalize="off"
            autoComplete="off"
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search channels..."
            value={query}
          />
        </div>
        <div className="max-h-60 overflow-y-auto p-1">
          {filtered.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-muted-foreground">
              No channels found.
            </p>
          ) : (
            filtered.map((channel) => (
              <button
                className={cn(
                  "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground",
                  channel.id === value && "bg-accent/50",
                )}
                key={channel.id}
                onClick={() => {
                  onChange(channel.id);
                  setOpen(false);
                  setQuery("");
                }}
                type="button"
              >
                <Check
                  className={cn(
                    "h-3.5 w-3.5 shrink-0",
                    channel.id === value ? "opacity-100" : "opacity-0",
                  )}
                />
                <span className="truncate">
                  {channel.name}{" "}
                  <span className="text-muted-foreground">
                    · {channel.channelType} · {channel.visibility}
                  </span>
                </span>
              </button>
            ))
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
