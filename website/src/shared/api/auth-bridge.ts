/**
 * Indirection between `shared/api/interceptors.ts` and `features/auth/store.ts`
 * — the API client and the auth store both want references to each other,
 * which is a circular dependency.
 *
 * Instead, `features/auth/init.ts` calls `registerAuth(getter, reauth)` once
 * at boot, and `shared/api/index.ts` reads them through this module. The
 * defaults reject so a misconfigured app surfaces a loud error rather than
 * silently calling `/api/*` unauthenticated.
 */

let getterFn: () => string | null = () => null;
let reauthFn: () => Promise<string> = () =>
  Promise.reject(new Error("auth not initialised (call registerAuth from initAuth)"));

export function registerAuth(g: () => string | null, r: () => Promise<string>): void {
  getterFn = g;
  reauthFn = r;
}

export const getStoredToken = (): string | null => getterFn();
export const reauthenticate = (): Promise<string> => reauthFn();
