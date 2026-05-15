import { Tabs as KTabs } from "@kobalte/core/tabs";
import { type ComponentProps, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export const Tabs = KTabs;

export function TabsList(props: ComponentProps<typeof KTabs.List>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <KTabs.List
      class={cn(
        "inline-flex h-9 items-center justify-center rounded-lg bg-muted p-1 text-muted-foreground",
        local.class,
      )}
      {...rest}
    >
      {local.children}
    </KTabs.List>
  );
}

export function TabsTrigger(props: ComponentProps<typeof KTabs.Trigger>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <KTabs.Trigger
      class={cn(
        "inline-flex items-center justify-center whitespace-nowrap rounded-md px-3 py-1 text-sm font-medium transition-all",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
        "ui-selected:bg-background ui-selected:text-foreground ui-selected:shadow",
        local.class,
      )}
      {...rest}
    >
      {local.children}
    </KTabs.Trigger>
  );
}

export function TabsContent(props: ComponentProps<typeof KTabs.Content>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <KTabs.Content
      class={cn(
        "mt-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        local.class,
      )}
      {...rest}
    >
      {local.children}
    </KTabs.Content>
  );
}
