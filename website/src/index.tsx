/* @refresh reload */
import { render } from "solid-js/web";
import "./index.css";

import { AppRouter } from "@/app/router";
import { initAuth } from "@/features/auth/init";
import { initI18n } from "@/shared/i18n/i18n";
import { initTheme } from "@/shared/lib/theme";

async function bootstrap() {
  initTheme();
  await initI18n();
  await initAuth();
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root mount node");
  render(() => <AppRouter />, root);
}

void bootstrap();
