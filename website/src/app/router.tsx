import { Navigate, Route, Router } from "@solidjs/router";
import { lazy } from "solid-js";
import { isInWebApp } from "@/shared/lib/telegram-webapp";
import { RootLayout } from "./layouts/root-layout";
import { WebappLayout } from "./layouts/webapp-layout";

const importHome = () => import("@/pages/home");
const Home = lazy(importHome);
const preloadHome = () => void importHome();

const importLogin = () => import("@/pages/login");
const Login = lazy(importLogin);
const preloadLogin = () => void importLogin();

const importChat = () => import("@/pages/chat");
const Chat = lazy(importChat);
const preloadChat = () => void importChat();

const importChatSettings = () => import("@/pages/chat-settings");
const ChatSettings = lazy(importChatSettings);
const preloadChatSettings = () => void importChatSettings();

const importChatAudit = () => import("@/pages/chat-audit");
const ChatAudit = lazy(importChatAudit);
const preloadChatAudit = () => void importChatAudit();

const importChatVerified = () => import("@/pages/chat-verified");
const ChatVerified = lazy(importChatVerified);
const preloadChatVerified = () => void importChatVerified();

const importChatBanned = () => import("@/pages/chat-banned");
const ChatBanned = lazy(importChatBanned);
const preloadChatBanned = () => void importChatBanned();

const importNotFound = () => import("@/pages/not-found");
const NotFound = lazy(importNotFound);
const preloadNotFound = () => void importNotFound();

export function AppRouter() {
  const Layout = isInWebApp() ? WebappLayout : RootLayout;
  return (
    <Router root={Layout}>
      <Route path="/" component={Home} preload={preloadHome} />
      <Route path="/login" component={Login} preload={preloadLogin} />
      <Route path="/chats/:chatId" component={Chat} preload={preloadChat}>
        <Route path="/" component={() => <Navigate href="settings" />} />
        <Route path="/settings" component={ChatSettings} preload={preloadChatSettings} />
        <Route path="/audit" component={ChatAudit} preload={preloadChatAudit} />
        <Route path="/verified" component={ChatVerified} preload={preloadChatVerified} />
        <Route path="/banned" component={ChatBanned} preload={preloadChatBanned} />
      </Route>
      <Route path="*" component={NotFound} preload={preloadNotFound} />
    </Router>
  );
}
