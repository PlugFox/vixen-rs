import { Separator as KSeparator } from "@kobalte/core/separator";
import { type ComponentProps, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export type SeparatorProps = ComponentProps<typeof KSeparator>;

export function Separator(props: SeparatorProps) {
  const [local, rest] = splitProps(props, ["class", "orientation"]);
  return (
    <KSeparator
      orientation={local.orientation ?? "horizontal"}
      class={cn(
        "shrink-0 bg-border",
        local.orientation === "vertical" ? "h-full w-px" : "h-px w-full",
        local.class,
      )}
      {...rest}
    />
  );
}
