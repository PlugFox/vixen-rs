import { describe, expect, it } from "vitest";
import { t } from "./i18n";
import type { MessageDef } from "./types";

describe("t()", () => {
  it("falls back to the English source when no RU translation is loaded", () => {
    const def: MessageDef = { ns: "common", key: "save", en: "Save" };
    expect(t(def)).toBe("Save");
  });

  it("interpolates {name} parameters", () => {
    const def: MessageDef = { ns: "x", key: "hello", en: "Hello {name}!" };
    expect(t(def, { name: "world" })).toBe("Hello world!");
  });

  it("leaves unknown placeholders untouched", () => {
    const def: MessageDef = { ns: "x", key: "y", en: "{a} + {b}" };
    expect(t(def, { a: "1" })).toBe("1 + {b}");
  });
});
