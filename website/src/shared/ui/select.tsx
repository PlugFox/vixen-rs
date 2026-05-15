import { Select as KSelect } from "@kobalte/core/select";
import { type JSX, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

export interface SelectProps<T extends string> {
  value: T | undefined;
  onChange: (v: T) => void;
  options: SelectOption<T>[];
  placeholder?: string;
  class?: string;
  disabled?: boolean;
  "aria-label"?: string;
}

/**
 * Thin Kobalte-Select wrapper with our visual primitives. The styling
 * mirrors the Input control so a Select sitting beside an Input feels
 * consistent.
 */
export function Select<T extends string>(props: SelectProps<T>) {
  return (
    <KSelect<SelectOption<T>>
      value={props.options.find((o) => o.value === props.value) ?? null}
      onChange={(opt) => {
        if (opt) props.onChange(opt.value);
      }}
      options={props.options}
      optionValue="value"
      optionTextValue="label"
      optionDisabled="disabled"
      placeholder={props.placeholder}
      disabled={props.disabled}
      itemComponent={(p) => (
        <KSelect.Item
          item={p.item}
          class="relative flex w-full cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none ui-highlighted:bg-accent ui-highlighted:text-accent-foreground ui-disabled:pointer-events-none ui-disabled:opacity-50"
        >
          <KSelect.ItemLabel>{p.item.rawValue.label}</KSelect.ItemLabel>
        </KSelect.Item>
      )}
    >
      <KSelect.Trigger
        class={cn(
          "flex h-9 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm",
          "placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring",
          "disabled:cursor-not-allowed disabled:opacity-50",
          props.class,
        )}
        aria-label={props["aria-label"]}
      >
        <KSelect.Value<SelectOption<T>>>{(state) => state.selectedOption()?.label}</KSelect.Value>
        <ChevronIcon />
      </KSelect.Trigger>
      <KSelect.Portal>
        <KSelect.Content class="relative z-50 max-h-96 min-w-[8rem] overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md">
          <KSelect.Listbox class="p-1" />
        </KSelect.Content>
      </KSelect.Portal>
    </KSelect>
  );
}

function ChevronIcon(): JSX.Element {
  return (
    <svg
      class="h-4 w-4 opacity-50"
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <polyline points="6 9 12 15 18 9" />
    </svg>
  );
}

const _SwitchPropsUnused = splitProps as unknown;
void _SwitchPropsUnused;
