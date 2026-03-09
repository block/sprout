import { CircleDot, Hash, Lock, Plus } from "lucide-react";

import {
  sidebarSections,
  type Channel,
  type ChannelSection,
} from "@/features/sidebar/data/sidebarData";
import { ThemeToggle } from "@/shared/theme/ThemeToggle";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInput,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarSeparator,
} from "@/shared/ui/sidebar";

function SidebarChannelIcon({ variant }: Pick<Channel, "variant">) {
  if (variant === "private") {
    return <Lock className="h-4 w-4" />;
  }

  if (variant === "direct") {
    return <CircleDot className="h-4 w-4" />;
  }

  return <Hash className="h-4 w-4" />;
}

function SidebarChannelItem({ channel }: { channel: Channel }) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        isActive={channel.active}
        tooltip={channel.name}
        type="button"
      >
        <SidebarChannelIcon variant={channel.variant} />
        <span>{channel.name}</span>
      </SidebarMenuButton>
      {channel.unread ? (
        <SidebarMenuBadge>{channel.unread}</SidebarMenuBadge>
      ) : null}
    </SidebarMenuItem>
  );
}

function SidebarChannelSection({ section }: { section: ChannelSection }) {
  return (
    <SidebarGroup>
      <SidebarGroupLabel>{section.title}</SidebarGroupLabel>
      <SidebarGroupAction aria-label={`Add ${section.title}`} type="button">
        <Plus className="h-4 w-4" />
      </SidebarGroupAction>
      <SidebarGroupContent>
        <SidebarMenu>
          {section.items.map((channel) => (
            <SidebarChannelItem
              channel={channel}
              key={`${section.title}-${channel.name}`}
            />
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

export function AppSidebar() {
  return (
    <Sidebar collapsible="offcanvas" variant="sidebar">
      <SidebarHeader className="gap-3">
        <div className="flex items-center gap-3 rounded-xl bg-sidebar-accent/80 px-3 py-3">
          <div className="flex h-6 w-6 items-center justify-center rounded-xl text-lg">
            <span aria-hidden="true">🌱</span>
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">Sprout</p>
            <p className="truncate text-xs text-sidebar-foreground/65">
              Humans and agents, together
            </p>
          </div>
        </div>
        <SidebarInput placeholder="Jump to channel" />
      </SidebarHeader>

      <SidebarSeparator />

      <SidebarContent>
        {sidebarSections.map((section) => (
          <SidebarChannelSection key={section.title} section={section} />
        ))}
      </SidebarContent>

      <SidebarFooter className="items-end">
        <ThemeToggle className="text-sidebar-foreground/65 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground" />
      </SidebarFooter>
    </Sidebar>
  );
}
