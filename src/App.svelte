<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Auth from "./lib/Auth.svelte";
  import Timer from "./lib/Timer.svelte";
  import { EMPTY, type TimerState } from "./lib/timer";

  let hasToken = $state<boolean | null>(null);
  let timer: TimerState = $state(EMPTY);

  $effect(() => {
    invoke<boolean>("has_api_token")
      .then((v) => (hasToken = v))
      .catch(() => (hasToken = false));

    const un = listen<TimerState>("timer-state", (e) => (timer = e.payload));
    return () => un.then((f) => f());
  });
</script>

{#if hasToken === null}
  <main class="shell">
    <section class="card">
      <p class="muted">Loading…</p>
    </section>
  </main>
{:else if !hasToken}
  <Auth onconnected={() => (hasToken = true)} />
{:else}
  <Timer {timer} />
{/if}
