import { getStoredToken, reauthenticate } from "./auth-bridge";
import { createApiClient } from "./client";
import { createAuthInterceptor, requestIdInterceptor } from "./interceptors";

/**
 * Singleton API client. The auth interceptor reads the JWT from
 * `auth-bridge`, which `features/auth/init.ts` populates on boot — every
 * `api.get / post / patch / del` automatically carries the Bearer token.
 */
export const api = createApiClient({
  baseUrl: __API_URL__,
  interceptors: [requestIdInterceptor, createAuthInterceptor(getStoredToken, reauthenticate)],
});

export type { ApiEnvelope, ApiErrorBody, ApiSuccess } from "./types";
export { ApiError, NetworkError } from "./types";
