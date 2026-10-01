<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { fmtDuration, type TimerState } from "./timer";

  let { timer }: { timer: TimerState } = $props();

  let now = $state(Date.now());
  let draft = $state("");
  let busy = $state(false);

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    const un = listen<string>("resume-requested", (e) => {
      if (!timer.running) draft = e.payload;
    });
    return () => {
      clearInterval(id);
      un.then((f) => f());
    };
  });

  const elapsed = $derived(
    timer.running && timer.startedAt
      ? timer.todaySeconds +
          Math.max(0, Math.floor((now - Number(timer.startedAt) * 1000) / 1000))
      : timer.todaySeconds,
  );

  async function start() {
    busy = true;
    try {
      await invoke("start_timer", { description: draft });
      draft = "";
    } finally {
      busy = false;
    }
  }

  async function stop() {
    busy = true;
    try {
      await invoke("stop_timer");
    } finally {
      busy = false;
    }
  }
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
      <button class="btn stop" disabled={busy} onclick={stop}>■ Stop</button>
    {:else}
      <div class="desc muted">
        {timer.status === "verifying" ? "Connecting…" : "No timer running"}
      </div>
      <div class="clock">{fmtDuration(timer.todaySeconds)} today</div>
      <input
        class="input desc-input"
        type="text"
        placeholder="What are you working on?"
        bind:value={draft}
        disabled={busy}
        onkeydown={(e) => e.key === "Enter" && !busy && start()}
      />
      <button
        class="btn primary"
        disabled={busy || timer.status !== "connected"}
        onclick={start}
      >
        ▶ Start
      </button>
    {/if}
  </section>
</main>
