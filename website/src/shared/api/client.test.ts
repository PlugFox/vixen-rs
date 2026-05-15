import { afterEach, describe, expect, it, vi } from "vitest";
import { createApiClient } from "./client";
import { ApiError } from "./types";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("createApiClient", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("unwraps the success envelope", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ status: "ok", data: { x: 1 } })),
    );
    const api = createApiClient({ baseUrl: "http://localhost/api" });
    const result = await api.get<{ x: number }>("/test");
    expect(result.x).toBe(1);
  });

  it("throws ApiError on the error envelope", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({ status: "error", error: { code: "FORBIDDEN", message: "nope" } }, 403),
      ),
    );
    const api = createApiClient({ baseUrl: "http://localhost/api" });
    await expect(api.get("/test")).rejects.toBeInstanceOf(ApiError);
  });

  it("interceptors compose left-to-right and bottom-up", async () => {
    const calls: string[] = [];
    const fetchMock = vi.fn(async () => jsonResponse({ status: "ok", data: null }));
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [
        async (req, next) => {
          calls.push("a-before");
          const r = await next(req);
          calls.push("a-after");
          return r;
        },
        async (req, next) => {
          calls.push("b-before");
          const r = await next(req);
          calls.push("b-after");
          return r;
        },
      ],
    });
    await api.get("/x");
    expect(calls).toEqual(["a-before", "b-before", "b-after", "a-after"]);
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});
