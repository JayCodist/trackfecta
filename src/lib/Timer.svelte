<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Entries from "./Entries.svelte";
  import Settings from "./Settings.svelte";
  import Icons from "./Icons.svelte";
  import {
    dotColor,
    faint,
    fmtDuration,
    fmtWait,
    type EntryRow,
    type LastEntry,
    type ProjectOption,
    type TimerState,
  } from "./timer";

  let { timer }: { timer: TimerState } = $props();

  let now = $state(Date.now());
  let draft = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  // The editable description of the RUNNING entry. While the input has no
  // focus, it copies the running row. The user can click it to rename the
  // entry while it runs.
  let runDesc = $state("");
  let runDescFocused = $state(false);

  let tab = $state<"list" | "settings">("list");

  // The picker data. It comes from the cached workspace project, client, and
  // tag lists. Reading the cache uses no requests. The lists refresh on
  // connect.
  let projects = $state<ProjectOption[]>([]);
  let allTags = $state<string[]>([]);
  let selProject = $state<number | null>(null);
  let selTags = $state<string[]>([]);
  let billable = $state(false);
  let openPop = $state<"" | "project" | "tag">("");
  let tagQuery = $state("");
  let projQuery = $state("");

  // Suggestions from recent entries, under the description input.
  let showSug = $state(false);

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    loadPickers();
    const unPickers = listen("pickers-changed", () => loadPickers());
    const unResume = listen<LastEntry | null>("resume-requested", (e) => {
      if (timer.running || !e.payload) return;
      const last = e.payload;
      draft = last.description ?? "";
      selProject = last.projectId;
      selTags = last.tags ?? [];
      billable = last.billable;
    });
    // App-local quick start: Ctrl+D (or Cmd+D) starts the timer with what
    // is in the bar. Works on Wayland, where global shortcuts cannot be
    // registered. Only while this window has focus.
    const onKey = (e: KeyboardEvent) => {
      if (
        (e.ctrlKey || e.metaKey) &&
        !e.altKey &&
        !e.shiftKey &&
        (e.key === "d" || e.key === "D")
      ) {
        e.preventDefault();
        if (!timer.running && !busy) void start();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      clearInterval(id);
      window.removeEventListener("keydown", onKey);
      unPickers.then((f) => f());
      unResume.then((f) => f());
    };
  });

  async function loadPickers() {
    try {
      projects = await invoke<ProjectOption[]>("get_picker_projects");
      allTags = await invoke<string[]>("get_picker_tags");
    } catch {
      /* The cache may be unavailable. Then the pickers simply stay empty. */
    }
  }

  // Prefill the project picker from the default in Settings. Once, on the
  // first idle load.
  let appliedDefault = false;
  $effect(() => {
    if (appliedDefault || timer.running || selProject !== null) return;
    if (projects.length === 0) return;
    appliedDefault = true;
    invoke<{ defaultProjectId: number | null }>("get_settings")
      .then((s) => {
        if (s.defaultProjectId !== null && !timer.running)
          selProject = s.defaultProjectId;
      })
      .catch(() => {});
  });

  /** The running row, when the timer is going. */
  const runningEntry = $derived(
    timer.running ? (timer.entries.find((e) => e.stop === null) ?? null) : null,
  );

  // The picker values in effect. While running, they copy the running row, so
  // the chips show what is active and edits target it. While idle, they are
  // the draft values.
  const curProjectId = $derived(runningEntry ? runningEntry.projectId : selProject);
  const curTags = $derived(runningEntry ? runningEntry.tags : selTags);
  const curBillable = $derived(runningEntry ? runningEntry.billable : billable);

  const selectedProject = $derived(
    projects.find((p) => p.id === curProjectId) ?? null,
  );

  // The timer bar shows ONLY the elapsed time of the running entry (zeros when
  // nothing is tracked). The day total is in the tray icon and the day
  // headers.
  const elapsed = $derived(
    timer.running && timer.startedAt
      ? Math.max(0, Math.floor((now - Number(timer.startedAt) * 1000) / 1000))
      : 0,
  );

  // Keep the editable running description in sync with the authoritative row
  // while the user is not typing in it. This happens right after Start, or
  // when a push brings a normalized description from the server.
  $effect(() => {
    if (!runDescFocused) runDesc = runningEntry?.description ?? "";
  });

  /** Commits a live rename of the running entry (blur or Enter). */
  function commitRunDesc() {
    runDescFocused = false;
    const cur = runningEntry?.description ?? "";
    if (runDesc.trim() !== cur.trim()) void applyChange({ description: runDesc });
  }

  const tagOptions = $derived(
    allTags.filter((t) => t.toLowerCase().includes(tagQuery.toLowerCase())),
  );

  const projOptions = $derived.by(() => {
    const q = projQuery.trim().toLowerCase();
    const list = q
      ? projects.filter(
          (p) =>
            p.name.toLowerCase().includes(q) ||
            (p.clientName ?? "").toLowerCase().includes(q),
        )
      : projects;
    return list.slice(0, 40);
  });

  /** The recent entries with distinct descriptions, matching the current
   * draft. Newest first. */
  const suggestions = $derived.by(() => {
    const q = draft.trim().toLowerCase();
    const seen = new Set<string>();
    const out: EntryRow[] = [];
    for (const e of timer.entries) {
      if (e.stop === null) continue; // skip the running row
      const d = (e.description ?? "").trim();
      if (!d || seen.has(d.toLowerCase())) continue;
      if (q && !d.toLowerCase().includes(q)) continue;
      seen.add(d.toLowerCase());
      out.push(e);
      if (out.length >= 6) break;
    }
    return out;
  });

  function pickSuggestion(e: EntryRow) {
    draft = e.description ?? "";
    selProject = e.projectId;
    selTags = e.tags;
    billable = e.billable;
    showSug = false;
  }

  /** Applies a picker change. While running, edit the live row. Otherwise set
   * the draft. */
  async function applyChange(patch: {
    description?: string;
    projectId?: number | null;
    tags?: string[];
    billable?: boolean;
  }) {
    if (!runningEntry) return;
    const next = {
      description: runningEntry.description ?? "",
      projectId: runningEntry.projectId,
      tags: runningEntry.tags,
      billable: runningEntry.billable,
      ...patch,
    };
    error = null;
    try {
      await invoke("update_entry", {
        id: runningEntry.id,
        description: next.description,
        start: runningEntry.start,
        stop: null,
        tags: next.tags,
        projectId: next.projectId,
        billable: next.billable,
      });
    } catch (e) {
      error = String(e);
    }
  }

  function toggleProject(id: number | null) {
    openPop = "";
    projQuery = "";
    if (runningEntry) {
      void applyChange({ projectId: curProjectId === id ? null : id });
      return;
    }
    selProject = selProject === id ? null : id;
  }

  function toggleTag(t: string) {
    tagQuery = "";
    if (runningEntry) {
      const has = curTags.includes(t);
      void applyChange({
        tags: has ? curTags.filter((x) => x !== t) : [...curTags, t],
      });
      return;
    }
    selTags = selTags.includes(t)
      ? selTags.filter((x) => x !== t)
      : [...selTags, t];
  }

  function addTag() {
    const t = tagQuery.trim();
    if (!t) return;
    if (!curTags.includes(t)) toggleTag(t);
    tagQuery = "";
  }

  function toggleBillable() {
    if (runningEntry) {
      void applyChange({ billable: !curBillable });
      return;
    }
    billable = !billable;
  }

  async function start() {
    busy = true;
    error = null;
    showSug = false;
    try {
      await invoke("start_timer", {
        description: draft,
        projectId: selProject,
        tags: selTags,
        billable,
      });
      draft = "";
      selProject = null;
      selTags = [];
      billable = false;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function stop() {
    busy = true;
    error = null;
    try {
      await invoke("stop_timer");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function closePop() {
    openPop = "";
    projQuery = "";
  }
</script>

<div class="app">
  <section class="timerbar">
    <div class="timerbar-row">
      {#if timer.running}
        <input
          class="desc-input"
          type="text"
          placeholder="Untitled"
          aria-label="Rename running entry"
          bind:value={runDesc}
          onfocus={() => (runDescFocused = true)}
          onblur={commitRunDesc}
          onkeydown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            else if (e.key === "Escape") {
              runDesc = runningEntry?.description ?? "";
              e.currentTarget.blur();
            }
          }}
        />
        <span class="timer-text running">{fmtDuration(elapsed)}</span>
        <button
          class="play-btn stop-btn"
          disabled={busy}
          title="Stop timer"
          aria-label="Stop timer"
          onclick={stop}
        >
          <Icons name="stop" />
        </button>
      {:else}
        <div class="desc-anchor">
          <input
            class="desc-input"
            type="text"
            placeholder="What are you working on?"
            bind:value={draft}
            disabled={busy}
            autocomplete="off"
            onfocus={() => (showSug = true)}
            oninput={() => (showSug = true)}
            onblur={() => setTimeout(() => (showSug = false), 150)}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                if (!busy) start();
              } else if (e.key === "Escape") {
                showSug = false;
              }
            }}
          />
          {#if showSug && suggestions.length > 0}
            <div class="pop sugg">
              <div class="pop-title">Recent entries</div>
              <div class="pop-list">
                {#each suggestions as s (s.id)}
                  <button
                    class="pop-item sugg-item"
                    onmousedown={(e) => e.preventDefault()}
                    onclick={() => pickSuggestion(s)}
                  >
                    <span class="sugg-col">
                      <span class="name">{s.description}</span>
                      {#if s.projectName}
                        <span class="sugg-sub">
                          <span
                            class="dot"
                            style="background:{dotColor(s.projectColor, s.projectName)}"
                          ></span>
                          {s.projectName}
                          {#if s.clientName}
                            <span class="sugg-client">• {s.clientName}</span>
                          {/if}
                        </span>
                      {/if}
                    </span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>
        <span class="timer-text">{fmtDuration(elapsed)}</span>
        <button
          class="play-btn"
          disabled={busy || timer.status !== "connected"}
          title="Start timer"
          aria-label="Start timer"
          onclick={start}
        >
          <Icons name="play" />
        </button>
      {/if}
    </div>

    <!-- The pickers are always available. While idle, they set the draft
         values for the next entry. While running, they edit the running row
         at once, and the push is queued. -->
    <div class="picker-row">
      <div class="pop-anchor">
        <button
          class="chip-btn"
          class:open={openPop === "project"}
          class:selected={selectedProject}
          style={selectedProject
            ? `--chip-bg:${faint(dotColor(selectedProject.color, selectedProject.name))}`
            : undefined}
          title="Project"
          aria-label="Project"
          onclick={() =>
            (openPop = openPop === "project" ? "" : "project")}
        >
          <Icons name="folder" />
          {#if selectedProject}
            {@const c = dotColor(selectedProject.color, selectedProject.name)}
            <span class="proj-chip" style="color:{c}">
              <span class="dot" style="background:{c}"></span>
              <span class="label">{selectedProject.name}</span>
              {#if selectedProject.clientName}
                <span class="chip-sep">•</span>
                <span class="chip-client">{selectedProject.clientName}</span>
              {/if}
            </span>
          {:else}
            <span class="label muted-label">Project</span>
          {/if}
        </button>
        {#if openPop === "project"}
          <div class="pop">
            <div class="pop-title">Projects</div>
            {#if projects.length > 8}
              <input
                class="pop-input"
                type="text"
                placeholder="Search projects…"
                bind:value={projQuery}
              />
            {/if}
            <div class="pop-list">
              {#if projects.length === 0}
                <div class="pop-empty">No projects yet.</div>
              {:else if projOptions.length === 0}
                <div class="pop-empty">No match for “{projQuery}”.</div>
              {:else}
                {#each projOptions as p (p.id)}
                  <button class="pop-item" onclick={() => toggleProject(p.id)}>
                    <span
                      class="dot"
                      style="background:{dotColor(p.color, p.name)}"
                    ></span>
                    <span class="sugg-col">
                      <span class="name">{p.name}</span>
                      {#if p.clientName}
                        <span class="sugg-sub">{p.clientName}</span>
                      {/if}
                    </span>
                    {#if curProjectId === p.id}
                      <span class="pop-check">✓</span>
                    {/if}
                  </button>
                {/each}
              {/if}
            </div>
            {#if curProjectId !== null}
              <button class="pop-clear" onclick={() => toggleProject(null)}>
                Clear project
              </button>
            {/if}
          </div>
        {/if}
      </div>

      <div class="pop-anchor">
        <button
          class="chip-btn"
          class:open={openPop === "tag"}
          class:selected={curTags.length > 0}
          title="Tags"
          aria-label="Tags"
          onclick={() => (openPop = openPop === "tag" ? "" : "tag")}
        >
          <Icons name="tag" />
          {#if curTags.length}
            <span class="label">{curTags.join(", ")}</span>
          {:else}
            <span class="label muted-label">Tags</span>
          {/if}
        </button>
        {#if openPop === "tag"}
          <div class="pop">
            <div class="pop-title">Tags</div>
            <input
              class="pop-input"
              type="text"
              placeholder="Add or search…"
              bind:value={tagQuery}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addTag();
                }
              }}
            />
            <div class="pop-list">
              {#each tagOptions as t (t)}
                <button class="pop-item" onclick={() => toggleTag(t)}>
                  <span class="name">{t}</span>
                  {#if curTags.includes(t)}
                    <span class="pop-check">✓</span>
                  {/if}
                </button>
              {:else}
                {#if tagQuery.trim()}
                  <div class="pop-empty">
                    Press Enter to add “{tagQuery.trim()}”.
                  </div>
                {:else}
                  <div class="pop-empty">No tags yet.</div>
                {/if}
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <button
        class="chip-btn"
        class:selected={curBillable}
        class:billable-on={curBillable}
        title="Billable"
        aria-pressed={curBillable}
        onclick={toggleBillable}
      >
        <Icons name="dollar" />
      </button>

      {#each curTags as t (t)}
        <span class="tag-chip">
          {t}
          <button aria-label="Remove {t}" onclick={() => toggleTag(t)}>
            ×
          </button>
        </span>
      {/each}
    </div>

    {#if error}
      <p class="error" style="padding:0 16px 10px">{error}</p>
    {/if}
  </section>

  <div class="tabs">
    <button
      class="tab"
      class:active={tab === "list"}
      onclick={() => (tab = "list")}
    >
      <Icons name="list" size={14} /> List
    </button>
    <button
      class="tab"
      class:active={tab === "settings"}
      onclick={() => (tab = "settings")}
    >
      <Icons name="gear" size={14} /> Settings
    </button>
  </div>

  {#if openPop}
    <button
      class="scrim"
      aria-label="Close menu"
      tabindex="-1"
      onclick={closePop}></button>
  {/if}

  {#if tab === "list"}
    <Entries {timer} {now} {projects} />
  {:else}
    <Settings {projects} {timer} />
  {/if}

  {#if timer.blocked || (timer.status === "connected" && timer.requestsLeft === 0)}
    <p class="quota">
      API quota reached. Next sync in {fmtWait(timer.nextSyncIn)}.
    </p>
  {/if}
</div>
