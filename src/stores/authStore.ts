import { create } from "zustand";
import { login as apiLogin, logout as apiLogout } from "../lib/api";

interface AuthState {
  user: { id: number; username: string; role: string } | null;
  token: string | null;
  login: (username: string, password: string) => Promise<void>;
  logout: () => void;
  isAdmin: () => boolean;
}

export const useAuthStore = create<AuthState>((set, get) => ({
  user: (() => {
    try {
      const stored = localStorage.getItem("auth_user");
      return stored ? JSON.parse(stored) : null;
    } catch {
      return null;
    }
  })(),
  token: localStorage.getItem("auth_token"),

  login: async (username: string, password: string) => {
    const res = await apiLogin(username, password);
    localStorage.setItem("auth_token", res.token);
    localStorage.setItem("auth_user", JSON.stringify(res.user));
    set({ token: res.token, user: res.user });
  },

  logout: () => {
    const { token } = get();
    if (token) {
      apiLogout(token).catch(() => {});
    }
    localStorage.removeItem("auth_token");
    localStorage.removeItem("auth_user");
    set({ token: null, user: null });
  },

  isAdmin: () => get().user?.role === "admin",
}));
