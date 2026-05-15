import { type JSX, splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export function Table(props: JSX.HTMLAttributes<HTMLTableElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <div class="relative w-full overflow-auto">
      <table class={cn("w-full caption-bottom text-sm", local.class)} {...rest}>
        {local.children}
      </table>
    </div>
  );
}

export function TableHeader(props: JSX.HTMLAttributes<HTMLTableSectionElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <thead class={cn("[&_tr]:border-b sticky top-0 bg-background", local.class)} {...rest}>
      {local.children}
    </thead>
  );
}

export function TableBody(props: JSX.HTMLAttributes<HTMLTableSectionElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <tbody class={cn("[&_tr:last-child]:border-0", local.class)} {...rest}>
      {local.children}
    </tbody>
  );
}

export function TableRow(props: JSX.HTMLAttributes<HTMLTableRowElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <tr
      class={cn(
        "border-b transition-colors hover:bg-muted/50 data-[state=selected]:bg-muted",
        local.class,
      )}
      {...rest}
    >
      {local.children}
    </tr>
  );
}

export function TableHead(props: JSX.ThHTMLAttributes<HTMLTableCellElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <th
      class={cn(
        "h-10 px-2 text-left align-middle text-xs font-medium text-muted-foreground",
        local.class,
      )}
      {...rest}
    >
      {local.children}
    </th>
  );
}

export function TableCell(props: JSX.TdHTMLAttributes<HTMLTableCellElement>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <td class={cn("p-2 align-middle [&:has([role=checkbox])]:pr-0", local.class)} {...rest}>
      {local.children}
    </td>
  );
}
