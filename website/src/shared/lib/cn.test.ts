import { describe, expect, it } from "vitest";
import { cn } from "./cn";

describe("cn", () => {
  it("joins truthy class names", () => {
    expect(cn("a", "b")).toBe("a b");
  });

  it("filters falsy values", () => {
    expect(cn("a", false && "b", null, undefined, "c")).toBe("a c");
  });

  it("merges conflicting Tailwind utilities — last wins", () => {
    expect(cn("bg-red-500", "bg-blue-500")).toBe("bg-blue-500");
    expect(cn("p-2 p-4")).toBe("p-4");
  });

  it("preserves non-conflicting utilities", () => {
    expect(cn("text-sm font-medium", "underline")).toContain("text-sm");
    expect(cn("text-sm font-medium", "underline")).toContain("font-medium");
    expect(cn("text-sm font-medium", "underline")).toContain("underline");
  });
});
