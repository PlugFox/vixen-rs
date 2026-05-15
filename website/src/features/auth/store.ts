import { createSignal } from "solid-js";
import { ApiError } from "@/shared/api";
import { getInitData, isInWebApp } from "@/shared/lib/telegram-webapp";
import { authApi } from "./api";
import type { LoginResponse, User } from "./types";

const [token, setToken] = createSignal<string | null>(null);
const [user, setUser] = createSignal<User | null>(null);
const [chatIds, setChatIds] = createSignal<number[]>([]);
const [loading, setLoading] = createSignal(true);

export const currentUser = user;
export const currentChatIds = chatIds;
export const isLoadingSession = loading;
export const isAuthenticated = () => token() !== null;
export const getToken = () => token();

function apply(resp: LoginResponse): void {
  setToken(resp.token);
  setUser(resp.user);
  setChatIds(resp.chat_ids);
}

export async function signInWithInitData(initData: string): Promise<string> {
  const resp = await authApi.loginWithInitData(initData);
  apply(resp);
  return resp.token;
}

export function signOut(): void {
  setToken(null);
  setUser(null);
  setChatIds([]);
}

/**
 * 401-interceptor recovery path.
 *  - WebApp: re-submit the always-available raw initData. Silent.
 *  - Browser: signOut + reject; the protected route renders <LoginPrompt>
 *    so the user signs in again via the Login Widget.
 */
export async function reauth(): Promise<string> {
  if (isInWebApp()) {
    const data = getInitData();
    if (!data) {
      signOut();
      throw new ApiError(401, "MISSING_INIT_DATA", "Telegram initData missing");
    }
    return await signInWithInitData(data);
  }
  signOut();
  throw new ApiError(401, "REAUTH_REQUIRED", "Sign in again");
}

export function finishLoading(): void {
  setLoading(false);
}
