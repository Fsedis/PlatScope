import { tick } from "svelte";

export function revealElement(element: HTMLElement | null): void {
  if (!element?.isConnected) return;
  element.scrollIntoView({ block: "start" });
  element.focus({ preventScroll: true });
}

export async function revealCompactDetail(id: string, breakpoint: string): Promise<void> {
  await tick();
  if (window.matchMedia(breakpoint).matches) revealElement(document.getElementById(id));
}
