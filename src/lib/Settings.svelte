<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    disable as disableAutostart,
    enable as enableAutostart,
    isEnabled,
  } from "@tauri-apps/plugin-autostart";
  import { projectColor, applyTheme, dotColor, type ProjectOption, type TimerState } from "./timer";
  import Dropdown, { type DropdownOption } from "./Dropdown.svelte";

  let { projects, timer }: { projects: ProjectOption[]; timer: TimerState } =
    $props();

  let capDraft = $state("30");
  let savedCap = $state(30);
  let defaultProjectId = $state<number | null>(null);
  let autostart = $state(false);
  let autostartBusy = $state(false);

  let theme = $state("system");
  let traySeconds = $state(false);

  let tokenDraft = $state("");
  let tokenBusy = $state(false);
  let note = $state<string | null>(null);
  let error = $state<string | null>(null);

  // Diagnostics: the recent log lines, for bug reports and crash analysis.
  let diag = $state<{ logTail: string; logPath: string } | null>(null);
  let showLog = $state(false);

  async function loadDiagnostics() {
    showLog = !showLog;
    if (showLog && !diag) {
      try {
        diag = await invoke<{ logTail: string; logPath: string }>(
          "get_diagnostics",
        );
      } catch (e) {
        error = String(e);
      }
    }
  }

  function copyLog() {
    if (!diag) return;
    navigator.clipboard.writeText(`${diag.logPath}\n\n${diag.logTail}`).then(
      () => (note = "Log copied to clipboard."),
      () => (error = "Clipboard unavailable."),
    );
  }

  $effect(() => {
    invoke<{
      hourlyCap: number;
      defaultProjectId: number | null;
      theme: string;
      trayShowSeconds: boolean;
    }>("get_settings")
      .then((s) => {
        capDraft = String(s.hourlyCap);
        savedCap = s.hourlyCap;
        defaultProjectId = s.defaultProjectId;
        theme = s.theme ?? "system";
        traySeconds = s.trayShowSeconds ?? false;
      })
      .catch(() => {});
    isEnabled()
      .then((v) => (autostart = v))
      .catch(() => {});
  });

  async function saveCap() {
    const n = Number(capDraft);
    if (!Number.isFinite(n) || n < 5) {
      error = "Cap must be at least 5 requests/hour.";
      return;
    }
    error = null;
    try {
      await invoke("set_setting", { key: "hourly_cap", value: capDraft });
      savedCap = n;
      note = "Quota cap saved.";
    } catch (e) {
      error = String(e);
    }
  }

  const projectOptions = $derived<DropdownOption[]>([
    { value: "", label: "No project" },
    ...projects.map((p) => ({
      value: String(p.id),
      label: p.clientName ? `${p.name}, ${p.clientName}` : p.name,
      color: dotColor(p.color, p.name),
    })),
  ]);

  async function saveDefaultProject(id: number | null) {
    defaultProjectId = id;
    try {
      await invoke("set_setting", {
        key: "default_project",
        value: id === null ? null : String(id),
      });
      note = "Default project saved.";
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleAutostart() {
    autostartBusy = true;
    try {
      if (autostart) await disableAutostart();
      else await enableAutostart();
      autostart = !autostart;
    } catch (e) {
      error = String(e);
    } finally {
      autostartBusy = false;
    }
  }

  async function saveTheme(t: string) {
    theme = t;
    applyTheme(t); // live, before the round-trip
    try {
      await invoke("set_setting", { key: "theme", value: t });
      note = "Appearance saved.";
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleTraySeconds() {
    traySeconds = !traySeconds;
    try {
      await invoke("set_setting", {
        key: "tray_show_seconds",
        value: traySeconds ? "true" : "false",
      });
      note = traySeconds
        ? "Tray icon now shows seconds."
        : "Tray icon shows hours and minutes.";
    } catch (e) {
      traySeconds = !traySeconds;
      error = String(e);
    }
  }

  async function replaceToken() {
    tokenBusy = true;
    error = null;
    note = null;
    try {
      await invoke("set_api_token", { token: tokenDraft });
      tokenDraft = "";
      note = "Token updated. Reconnected.";
    } catch (e) {
      error = String(e);
    } finally {
      tokenBusy = false;
    }
  }

  async function logout() {
    try {
      await invoke("logout");
      // App.svelte swaps to the Auth screen when status flips to disconnected.
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="settings">
  {#if note}
    <p class="muted" style="margin:0;font-size:13px">{note}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div>
    <h3 class="set-title">Account</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">API token</div>
          <div class="set-hint">
            Stored in your system keyring. Find it on your
            <a
              class="link"
              href="https://track.toggl.com/profile"
              target="_blank"
              rel="noreferrer">profile page</a>.
          </div>
        </div>
        <div class="set-ctl">
          <input
            class="input"
            type="password"
            placeholder="New token…"
            bind:value={tokenDraft}
            style="width:150px"
          />
          <button
            class="btn ghost"
            disabled={!tokenDraft.trim() || tokenBusy}
            onclick={replaceToken}
          >
            {tokenBusy ? "Saving…" : "Replace"}
          </button>
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Sign out</div>
          <div class="set-hint">Forget the token on this device.</div>
        </div>
        <div class="set-ctl">
          <button class="btn ghost" onclick={logout}>Log out</button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Appearance</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Theme</div>
          <div class="set-hint">
            Follows your system preference by default; choose light or dark to
            pin it.
          </div>
        </div>
        <div class="set-ctl">
          <Dropdown
            options={[
              { value: "system", label: "System" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            value={theme}
            placeholder="Theme"
            icon=""
            onChange={(v) => saveTheme(v)}
          />
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Tray icon seconds</div>
          <div class="set-hint">
            The tray shows today's tracked time. Include seconds for a
            second-by-second readout while a timer runs.
          </div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={traySeconds}
            role="switch"
            aria-checked={traySeconds}
            aria-label="Show seconds in tray icon"
            onclick={toggleTraySeconds}
          ></button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Tracking</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Default project</div>
          <div class="set-hint">Prefills the project picker when idle.</div>
        </div>
        <div class="set-ctl">
          <Dropdown
            options={projectOptions}
            value={defaultProjectId === null ? "" : String(defaultProjectId)}
            placeholder="No project"
            onChange={(v) => saveDefaultProject(v ? Number(v) : null)}
          />
        </div>
      </div>
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Background sync</div>
          <div class="set-hint">
            Polls Toggl while the window is closed. Every request counts
            against your hourly cap.
          </div>
        </div>
        <div class="set-ctl">
          <span class="muted" style="font-size:12px">opportunistic</span>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">API budget</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Requests per hour</div>
          <div class="set-hint">
            Toggl limits API calls per rolling hour. Free plans get about 30,
            paid plans more. Background sync and every edit spend from this
            pool; raise the cap to match your plan and sync more aggressively.
          </div>
          <div class="quota-line">
            <span
              class="quota-dot"
              class:low={timer.status === "connected" && timer.requestsLeft <= 3}
            ></span>
            {timer.requestsLeft} left this hour (of {savedCap})
          </div>
        </div>
        <div class="set-ctl">
          <input
            class="input"
            type="number"
            min="5"
            max="2000"
            bind:value={capDraft}
            style="width:76px"
          />
          <button class="btn ghost" onclick={saveCap}>Save</button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Startup</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Launch at login</div>
          <div class="set-hint">Starts minimized to the tray.</div>
        </div>
        <div class="set-ctl">
          <button
            class="switch"
            class:on={autostart}
            role="switch"
            aria-checked={autostart}
            aria-label="Launch at login"
            disabled={autostartBusy}
            onclick={toggleAutostart}
          ></button>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Web</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Toggl Track</div>
          <div class="set-hint">Reports and timesheet live in the web app.</div>
        </div>
        <div class="set-ctl">
          <a
            class="link"
            href="https://track.toggl.com/timer"
            target="_blank"
            rel="noreferrer">Open Track</a>
          <a
            class="link"
            href="https://track.toggl.com/reports"
            target="_blank"
            rel="noreferrer">Reports</a>
        </div>
      </div>
    </div>
  </div>

  <div>
    <h3 class="set-title">Diagnostics</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">Recent activity log</div>
          <div class="set-hint">
            Sync attempts, API errors and crashes are recorded locally.
            Include the tail when you report a bug.
          </div>
        </div>
        <div class="set-ctl">
          <button class="btn ghost" onclick={loadDiagnostics}>
            {showLog ? "Hide" : "Show"}
          </button>
        </div>
      </div>
      {#if showLog}
        <div class="diag">
          {#if diag?.logPath}
            <div class="diag-path" title={diag.logPath}>{diag.logPath}</div>
          {/if}
          <pre class="diag-log">{diag?.logTail?.trim() || "(no log lines yet)"}</pre>
          <button class="btn ghost" onclick={copyLog}>Copy log</button>
        </div>
      {/if}
    </div>
  </div>

  <div>
    <h3 class="set-title">About</h3>
    <div class="set-group">
      <div class="set-row">
        <div class="set-main">
          <div class="set-label">
            ToggLinux <span class="muted" style="font-weight:400">v1.0.0</span>
          </div>
          <div class="set-hint">
            Unofficial Toggl Track desktop client for Linux. Not affiliated
            with, endorsed by, or sponsored by Toggl™.
          </div>
        </div>
        <div class="set-ctl">
          {#if projects.length > 0}
            <span
              class="dot"
              title="Recent projects loaded"
              style="background:{projectColor(projects[0].name)}"
            ></span>
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>
