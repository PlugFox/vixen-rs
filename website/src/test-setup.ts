import "@testing-library/jest-dom/vitest";
import { beforeEach } from "vitest";

// Clean localStorage between tests — the theme + locale modules read it on
// boot, and a leftover entry would leak state between cases.
beforeEach(() => {
  localStorage.clear();
});
