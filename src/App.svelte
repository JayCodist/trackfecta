<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Auth from "./lib/Auth.svelte";
  import Timer from "./lib/Timer.svelte";
  import IdleDialog from "./lib/IdleDialog.svelte";
  import { applyTheme, EMPTY, type IdlePending, type TimerState } from "./lib/timer";

  // Get the current snapshot on mount. Then keep it in sync through the
  // timer-state broadcast from the Rust side. This covers the startup race
  // where the first broadcast happens before the listener is attached.
  let timer: TimerState = $state(EMPTY);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  // An idle period that ended while a timer ran. The Rust side sends this
  // when the user returns. Null means no dialog is open.
  let idle = $state<IdlePending | null>(null);

  $effect(() => {
    // Apply the saved appearance choice before the first paint of content.
    // "system" means no attribute, so CSS follows prefers-color-scheme.
    invoke<{ theme: string }>("get_settings")
      .then((s) => applyTheme(s.theme))
      .catch(() => {});

    invoke<TimerState>("get_state")
      .then((s) => (timer = s))
      .catch(() => {});

    // A prompt can be waiting from before the listener existed. Pick it up.
    invoke<IdlePending | null>("get_idle_pending")
      .then((p) => {
        if (p) idle = p;
      })
      .catch(() => {});

    const unState = listen<TimerState>("timer-state", (e) => (timer = e.payload));
    const unIdle = listen<IdlePending>("idle-dialog", (e) => {
      // The monitor thread raises the window, but a hidden webview may
      // still be loading. Show the dialog whenever the state arrives.
      idle = e.payload;
    });
    const unToast = listen<string>("toast", (e) => {
      toast = e.payload;
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => (toast = null), 6000);
    });
    return () => {
      unState.then((f) => f());
      unIdle.then((f) => f());
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

{#if idle}
  <IdleDialog {idle} onClose={() => (idle = null)} />
{/if}

{#if toast}
  <div class="toast" role="alert">{toast}</div>
{/if}
