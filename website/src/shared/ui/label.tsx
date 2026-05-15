import { type JSX, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export type LabelProps = JSX.LabelHTMLAttributes<HTMLLabelElement>;

export function Label(props: LabelProps) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: generic Label primitive — caller wires `for` / nests the control.
    <label
      class={cn(
        "text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70",
        local.class,
      )}
      {...rest}
    >
      {local.children}
    </label>
  );
}
