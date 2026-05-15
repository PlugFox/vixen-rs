import { type ApiEnvelope, ApiError, NetworkError } from "./types";

export interface RequestOptions {
  signal?: AbortSignal;
  headers?: Record<string, string>;
}

export type FetchFn = (req: Request) => Promise<Response>;

/**
 * Interceptor signature. Each interceptor wraps the next link in the chain,
 * giving it a chance to:
 *  - inject headers / mutate the URL before `next(req)`
 *  - act on the response (retry on 401, etc.) after `await next(req)`
 *
 * The bottom of the chain is `fetch`.
 */
export type Interceptor = (req: Request, next: FetchFn) => Promise<Response>;

export interface ApiClientConfig {
  baseUrl: string;
  interceptors?: Interceptor[];
}

export interface ApiClient {
  get<T>(path: string, opts?: RequestOptions): Promise<T>;
  post<T>(path: string, body?: unknown, opts?: RequestOptions): Promise<T>;
  patch<T>(path: string, body?: unknown, opts?: RequestOptions): Promise<T>;
  del<T>(path: string, opts?: RequestOptions): Promise<T>;
}

function buildChain(interceptors: Interceptor[]): FetchFn {
  const base: FetchFn = (req) => fetch(req);
  return interceptors.reduceRight<FetchFn>((next, layer) => (req) => layer(req, next), base);
}

async function parseEnvelope<T>(res: Response): Promise<T> {
  let body: ApiEnvelope<T> | null = null;
  try {
    body = (await res.json()) as ApiEnvelope<T>;
  } catch (e) {
    if (!res.ok) {
      throw new ApiError(res.status, "BAD_RESPONSE", `HTTP ${res.status}`);
    }
    throw new NetworkError("response body is not valid JSON", e);
  }
  if (body && body.status === "ok") {
    return body.data;
  }
  if (body && body.status === "error") {
    throw new ApiError(res.status, body.error.code, body.error.message);
  }
  throw new NetworkError("response envelope is not recognised");
}

export function createApiClient(config: ApiClientConfig): ApiClient {
  const chain = buildChain(config.interceptors ?? []);

  async function request<T>(
    method: string,
    path: string,
    body: unknown,
    opts: RequestOptions | undefined,
  ): Promise<T> {
    const url = `${config.baseUrl}${path}`;
    const headers = new Headers(opts?.headers);
    if (body !== undefined) headers.set("content-type", "application/json");

    const req = new Request(url, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: opts?.signal,
    });

    let res: Response;
    try {
      res = await chain(req);
    } catch (e) {
      if (e instanceof ApiError || e instanceof NetworkError) throw e;
      throw new NetworkError("fetch failed", e);
    }
    return parseEnvelope<T>(res);
  }

  return {
    get: (path, opts) => request("GET", path, undefined, opts),
    post: (path, body, opts) => request("POST", path, body, opts),
    patch: (path, body, opts) => request("PATCH", path, body, opts),
    del: (path, opts) => request("DELETE", path, undefined, opts),
  };
}
