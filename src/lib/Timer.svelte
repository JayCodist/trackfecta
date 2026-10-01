<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fmtDuration, type TimerState } from "./timer";

  let { timer }: { timer: TimerState } = $props();

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const elapsed = $derived(
    timer.running && timer.startedAt
      ? timer.todaySeconds +
          Math.max(0, Math.floor((now - Number(timer.startedAt) * 1000) / 1000))
      : timer.todaySeconds,
  );
</script>

<main class="shell">
  <header class="topbar">
    <span class="brand">ToggLinux</span>
    <span class="muted">unofficial Toggl Track client</span>
  </header>

  <section class="card timer-card">
    {#if timer.running}
      <div class="desc">{timer.description ?? "Untitled"}</div>
      <div class="clock running">{fmtDuration(elapsed)}</div>
      <button class="btn stop" onclick={() => invoke("stop_timer")}>■ Stop</button>
    {:else}
      <div class="desc muted">No timer running</div>
      <div class="clock">{fmtDuration(timer.todaySeconds)} today</div>
      <button
        class="btn primary"
        onclick={() => invoke("start_timer", { description: "Quick entry" })}
      >
        ▶ Start
      </button>
    {/if}
  </section>
</main>
