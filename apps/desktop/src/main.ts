import { mount } from "svelte";

import App from "./App.svelte";
import "./app.css";

async function bootstrap(): Promise<void> {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("mock")) {
    const { installMarketBrowserMock } = await import("./lib/devMock");
    await installMarketBrowserMock();
  }

  const target = document.getElementById("app");
  if (!target) {
    throw new Error("PlatScope application root is missing");
  }

  mount(App, { target });
}

void bootstrap();
