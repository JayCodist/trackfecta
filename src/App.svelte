<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Auth from "./lib/Auth.svelte";
  import Timer from "./lib/Timer.svelte";
  import { applyTheme, EMPTY, type TimerState } from "./lib/timer";

  // Get the current snapshot on mount. Then keep it in sync through the
  // timer-state broadcast from the Rust side. This covers the startup race
  // where the first broadcast happens before the listener is attached.
  let timer: TimerState = $state(EMPTY);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    // Apply the saved appearance choice before the first paint of content.
    // "system" means no attribute, so CSS follows prefers-color-scheme.
    invoke<{ theme: string }>("get_settings")
      .then((s) => applyTheme(s.theme))
      .catch(() => {});

    invoke<TimerState>("get_state")
      .then((s) => (timer = s))
      .catch(() => {});

    const unState = listen<TimerState>("timer-state", (e) => (timer = e.payload));
    const unToast = listen<string>("toast", (e) => {
      toast = e.payload;
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => (toast = null), 6000);
    });
    return () => {
      unState.then((f) => f());
      unToast.then((f) => f());
      clearTimeout(toastTimer);
    };
  });
</script>

{#if timer.status === "disconnected"}
  <Auth />
{:else}
  <Timer {timer} />
{/if}

{#if toast}
  <div class="toast" role="alert">{toast}</div>
{/if}
