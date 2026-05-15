import { api } from "@/shared/api";
import type { LoginResponse, MeResponse } from "./types";

export const authApi = {
  loginWithInitData: (init_data: string) =>
    api.post<LoginResponse>("/auth/telegram/login", { init_data }),
  me: () => api.get<MeResponse>("/auth/me"),
};
