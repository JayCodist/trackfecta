import { mount } from "svelte";
import { openUrl } from "@tauri-apps/plugin-opener";
import App from "./App.svelte";
import "./styles.css";

// The Tauri webview cannot open a new window, so a target=_blank link does
// nothing, and a plain link would navigate the app itself. This delegated
// handler catches external clicks and hands the URL to the opener plugin,
// which launches the OS default browser. The window.open fallback only
// matters in a plain-browser preview, where the plugin is unavailable.
document.addEventListener("click", (event) => {
  const anchor = (event.target as Element | null)?.closest?.("a[href]");
  if (!anchor) return;
  const href = anchor.getAttribute("href") ?? "";
  if (!/^(https?:|mailto:|tel:)/i.test(href)) return;
  event.preventDefault();
  try {
    openUrl(href).catch(() => window.open(href, "_blank", "noopener"));
  } catch {
    window.open(href, "_blank", "noopener");
  }
});

const app = mount(App, {
  target: document.getElementById("root")!,
});

export default app;
