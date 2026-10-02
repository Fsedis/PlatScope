import { mount } from "svelte";

import App from "./App.svelte";
import RewardOverlay from "./lib/RewardOverlay.svelte";
import RelicSelectionOverlay from "./lib/RelicSelectionOverlay.svelte";
import "./app.css";

async function bootstrap(): Promise<void> {
  const query = new URLSearchParams(window.location.search);
  const overlay = query.has("overlay");
  const relicSelectionOverlay = query.has("relic-selection-overlay");
  if (overlay || relicSelectionOverlay) document.documentElement.classList.add("overlay-mode");
  if (import.meta.env.DEV && query.has("mock")) {
    const { installMarketBrowserMock } = await import("./lib/devMock");
    await installMarketBrowserMock();
  }

  const target = document.getElementById("app");
  if (!target) {
    throw new Error("PlatScope application root is missing");
  }

  mount(relicSelectionOverlay ? RelicSelectionOverlay : overlay ? RewardOverlay : App, { target });
}

void bootstrap();
