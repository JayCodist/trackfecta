<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { onconnected }: { onconnected: () => void } = $props();

  let token = $state("");

  async function saveToken() {
    await invoke("set_api_token", { token: token.trim() });
    onconnected();
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
    <button class="btn primary" disabled={!token.trim()} onclick={saveToken}>
      Connect
    </button>
    <p class="fineprint">
      ToggLinux is an unofficial client and is not affiliated with, endorsed by,
      or sponsored by Toggl™.
    </p>
  </section>
</main>
