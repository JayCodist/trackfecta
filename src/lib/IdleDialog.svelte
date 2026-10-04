<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fmtShort, type IdlePending } from "./timer";

  let { idle, onClose }: { idle: IdlePending; onClose: () => void } =
    $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  async function answer(action: "keep" | "discard" | "split") {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await invoke("resolve_idle", { action });
      // The Rust side cleared the pending prompt. Close the dialog.
      onClose();
    } catch (e) {
      // Keep the dialog open so the user can retry.
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function onkey(e: KeyboardEvent) {
    // Escape means "do nothing", the same as Keep.
    if (e.key === "Escape") answer("keep");
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="idle-scrim"></div>
<div class="idle-card" role="alertdialog" aria-labelledby="idle-title">
  <h2 id="idle-title">You were idle</h2>
  <p class="idle-body">
    You were away for <strong>{fmtShort(idle.idleSeconds)}</strong>
    {#if idle.runningDescription}
      while "{idle.runningDescription}" was running.
    {:else}
      while a timer was running.
    {/if}
    What should happen to that time?
  </p>
  <p class="idle-range">
    Idle from {new Date(idle.idleStart * 1000).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    })} to {new Date(idle.idleEnd * 1000).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    })}.
  </p>
  {#if error}
    <p class="error">{error}</p>
  {/if}
  <div class="idle-actions">
    <button class="btn ghost" disabled={busy} onclick={() => answer("keep")}>
      Keep
    </button>
    <button class="btn ghost" disabled={busy} onclick={() => answer("discard")}>
      Discard
    </button>
    <button class="btn primary" disabled={busy} onclick={() => answer("split")}>
      Discard and Continue
    </button>
  </div>
</div>

<style>
  .idle-scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    z-index: 70;
  }
  .idle-card {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow-pop);
    padding: 22px 24px;
    width: min(420px, calc(100vw - 40px));
    z-index: 71;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    color: var(--text-strong);
  }
  .idle-body {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.5;
    color: var(--text);
  }
  .idle-range {
    margin: 0;
    font-size: 12.5px;
    color: var(--muted-2);
  }
  .idle-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
