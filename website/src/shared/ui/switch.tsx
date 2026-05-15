import { Switch as KSwitch } from "@kobalte/core/switch";
import { type ComponentProps, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export type SwitchProps = Omit<ComponentProps<typeof KSwitch>, "children">;

export function Switch(props: SwitchProps) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <KSwitch class={cn("inline-flex items-center gap-2", local.class)} {...rest}>
      <KSwitch.Input class="sr-only" />
      <KSwitch.Control class="peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent bg-input transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ui-disabled:cursor-not-allowed ui-disabled:opacity-50 ui-checked:bg-primary">
        <KSwitch.Thumb class="pointer-events-none block h-4 w-4 rounded-full bg-background shadow-lg ring-0 transition-transform translate-x-0 ui-checked:translate-x-4" />
      </KSwitch.Control>
    </KSwitch>
  );
}

export const SwitchLabel = KSwitch.Label;
