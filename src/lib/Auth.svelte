<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let token = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  // No callback needed: on success the Rust side flips `timer-state`.status to
  // "connected" and App.svelte swaps the view automatically.
  async function saveToken() {
    busy = true;
    error = null;
    try {
      await invoke("set_api_token", { token });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<main class="shell">
  <section class="card auth-card">
    <h1>Welcome to ToggLinux</h1>
    <p class="muted">
      Paste your Toggl&nbsp;Track API token to get started. You can find it on your
      <a href="https://track.toggl.com/profile" target="_blank" rel="noreferrer">
        profile page
      </a>. It is stored securely in your system keyring.
    </p>
    <input
      class="input"
      type="password"
      placeholder="API token"
      bind:value={token}
    />
    <button
      class="btn primary"
      disabled={!token.trim() || busy}
      onclick={saveToken}
    >
      {busy ? "Verifying…" : "Connect"}
    </button>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <p class="fineprint">
      ToggLinux is an unofficial client and is not affiliated with, endorsed by,
      or sponsored by Toggl™.
    </p>
  </section>
</main>
