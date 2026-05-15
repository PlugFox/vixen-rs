import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merge Tailwind class lists safely. `clsx` handles conditionals + falsy
 * values; `twMerge` resolves conflicting utilities (`bg-red-500 bg-blue-500`
 * → `bg-blue-500`) so caller-provided `class` props can override defaults
 * without the order anxiety.
 */
export const cn = (...inputs: ClassValue[]) => twMerge(clsx(inputs));
