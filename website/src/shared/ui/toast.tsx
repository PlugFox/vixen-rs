import { Toast as KToast, toaster } from "@kobalte/core/toast";
import { Portal } from "solid-js/web";
import { cn } from "@/shared/lib/cn";

export type ToastVariant = "default" | "success" | "destructive" | "warning";

interface ShowToastInput {
  title?: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
}

const variantClass: Record<ToastVariant, string> = {
  default: "bg-background text-foreground",
  success: "bg-success text-success-foreground",
  destructive: "bg-destructive text-destructive-foreground",
  warning: "bg-warning text-warning-foreground",
};

/**
 * Module-scope helper. Components call `showToast({...})`; the actual
 * `<ToastRegion>` is mounted exactly once from `RootLayout` so toasts are
 * available throughout the app.
 */
export function showToast(input: ShowToastInput): void {
  toaster.show((p) => (
    <KToast toastId={p.toastId} duration={input.duration ?? 4000}>
      <div
        class={cn(
          "flex w-80 flex-col gap-1 rounded-md border p-4 shadow-lg",
          variantClass[input.variant ?? "default"],
        )}
      >
        {input.title ? (
          <KToast.Title class="text-sm font-semibold">{input.title}</KToast.Title>
        ) : null}
        {input.description ? (
          <KToast.Description class="text-sm opacity-90">{input.description}</KToast.Description>
        ) : null}
      </div>
    </KToast>
  ));
}

export function ToastRegion() {
  return (
    <Portal>
      <KToast.Region>
        <KToast.List class="fixed bottom-4 right-4 z-[100] flex max-h-screen w-full flex-col-reverse gap-2 p-4 sm:bottom-4 sm:right-4 sm:top-auto sm:flex-col md:max-w-[420px]" />
      </KToast.Region>
    </Portal>
  );
}
