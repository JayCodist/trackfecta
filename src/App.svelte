<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Auth from "./lib/Auth.svelte";
  import Timer from "./lib/Timer.svelte";
  import { EMPTY, type TimerState } from "./lib/timer";

  // Fetch the current snapshot on mount, then keep it in sync via the Rust-side
  // `timer-state` broadcast (handles the startup race where the first broadcast
  // fires before this listener attaches).
  let timer: TimerState = $state(EMPTY);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
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
