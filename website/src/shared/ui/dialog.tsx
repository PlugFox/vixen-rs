import { Dialog as KDialog } from "@kobalte/core/dialog";
import { type ComponentProps, type JSX, type ParentProps, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export const Dialog = KDialog;
export const DialogTrigger = KDialog.Trigger;
export const DialogClose = KDialog.CloseButton;

export function DialogContent(props: ParentProps<{ class?: string }>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <KDialog.Portal>
      <KDialog.Overlay class="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm data-[expanded]:animate-in data-[expanded]:fade-in-0 data-[closed]:animate-out data-[closed]:fade-out-0" />
      <KDialog.Content
        class={cn(
          "fixed left-1/2 top-1/2 z-50 grid w-full max-w-lg -translate-x-1/2 -translate-y-1/2 gap-4 rounded-lg border bg-background p-6 shadow-lg duration-200",
          "data-[expanded]:animate-in data-[expanded]:fade-in-0 data-[expanded]:zoom-in-95",
          "data-[closed]:animate-out data-[closed]:fade-out-0 data-[closed]:zoom-out-95",
          local.class,
        )}
        {...rest}
      >
        {local.children}
        {/* The accessible name lives on the BUTTON, not the SVG inside — some
            screen readers ignore aria-label on non-interactive children. */}
        <KDialog.CloseButton
          aria-label="Close"
          class="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M18 6L6 18" />
            <path d="M6 6L18 18" />
          </svg>
        </KDialog.CloseButton>
      </KDialog.Content>
    </KDialog.Portal>
  );
}

export function DialogHeader(props: JSX.HTMLAttributes<HTMLDivElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <div class={cn("flex flex-col space-y-1.5 text-left", local.class)} {...rest}>
      {local.children}
    </div>
  );
}

export function DialogFooter(props: JSX.HTMLAttributes<HTMLDivElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <div
      class={cn("flex flex-col-reverse gap-2 sm:flex-row sm:justify-end", local.class)}
      {...rest}
    >
      {local.children}
    </div>
  );
}

export function DialogTitle(props: ComponentProps<typeof KDialog.Title>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <KDialog.Title
      class={cn("text-lg font-semibold leading-none tracking-tight", local.class)}
      {...rest}
    >
      {local.children}
    </KDialog.Title>
  );
}

export function DialogDescription(props: ComponentProps<typeof KDialog.Description>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <KDialog.Description class={cn("text-sm text-muted-foreground", local.class)} {...rest}>
      {local.children}
    </KDialog.Description>
  );
}
