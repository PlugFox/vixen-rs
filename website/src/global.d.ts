/// <reference types="vite/client" />

// Build-time constants injected by `vite.config.ts` `define`.
declare const __CLIENT_VERSION__: string;
declare const __BUILD_TIME__: string;
declare const __GIT_COMMIT__: string;
declare const __GIT_BRANCH__: string;
declare const __API_URL__: string;
/// Bot username for the Telegram Login Widget (`data-telegram-login`).
declare const __BOT_USERNAME__: string;

// ── Telegram WebApp typings ──────────────────────────────────────────────
//
// Minimal surface — we only consume what the dashboard actually needs. Full
// reference: https://core.telegram.org/bots/webapps#initializing-mini-apps
//
// CRITICAL: never trust `initDataUnsafe`. Always submit the raw signed
// `initData` string to the server for HMAC validation.

interface TelegramWebAppUser {
  id: number;
  first_name: string;
  last_name?: string;
  username?: string;
  language_code?: string;
  photo_url?: string;
}

interface TelegramWebAppBackButton {
  isVisible: boolean;
  show(): void;
  hide(): void;
  onClick(cb: () => void): void;
  offClick(cb: () => void): void;
}

type TelegramWebAppEventName =
  | "themeChanged"
  | "viewportChanged"
  | "mainButtonClicked"
  | "backButtonClicked";

interface TelegramWebApp {
  /** Raw, server-validated initData string. Never trust the parsed form. */
  initData: string;
  initDataUnsafe: {
    user?: TelegramWebAppUser;
    auth_date?: number;
    hash?: string;
    query_id?: string;
  };
  version: string;
  platform: string;
  colorScheme: "light" | "dark";
  themeParams: {
    bg_color?: string;
    text_color?: string;
    hint_color?: string;
    link_color?: string;
    button_color?: string;
    button_text_color?: string;
    secondary_bg_color?: string;
  };
  BackButton: TelegramWebAppBackButton;
  ready(): void;
  close(): void;
  expand(): void;
  onEvent(event: TelegramWebAppEventName, handler: () => void): void;
  offEvent(event: TelegramWebAppEventName, handler: () => void): void;
}

interface Window {
  Telegram?: {
    WebApp: TelegramWebApp;
  };
  /** Login Widget global callback. Set by `LoginWidget` on mount. */
  onTelegramAuth?: (user: TelegramLoginWidgetUser) => void;
  /** Runtime override for the bot username when build-time env is missing. */
  __BOT_USERNAME__?: string;
}

/** Payload returned by the Telegram Login Widget JS callback. */
interface TelegramLoginWidgetUser {
  id: number;
  first_name: string;
  last_name?: string;
  username?: string;
  photo_url?: string;
  auth_date: number;
  hash: string;
}
