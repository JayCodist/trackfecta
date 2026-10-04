<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Icons from "./Icons.svelte";
  import Dropdown, { type DropdownOption } from "./Dropdown.svelte";
  import {
    dayLabel,
    dotColor,
    fmtClock,
    fmtDayTotal,
    fmtDuration,
    startOfDay,
    todayAt,
    toTimeValue,
    type EntryRow,
    type ProjectOption,
    type TimerState,
  } from "./timer";

  let { timer, now, projects }: { timer: TimerState; now: number; projects: ProjectOption[] } =
    $props();

  let editing = $state<number | null>(null);
  let draftDesc = $state("");
  let draftTags = $state("");
  let draftStart = $state("");
  let draftStop = $state("");
  let draftProject = $state<number | null>(null);
  let draftBillable = $state(false);

  let showManual = $state(false);
  let manDesc = $state("");
  let manTags = $state("");
  let manStart = $state("09:00");
  let manStop = $state("10:00");
  let manProject = $state<number | null>(null);
  let manBillable = $state(false);
  let error = $state<string | null>(null);

  /** The keys of the day groups that are collapsed. Empty means all are
   * expanded. */
  let collapsed = $state<Set<string>>(new Set());

  function toggleGroup(key: string) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }

  /** The entries grouped by local day, newest day first. The list is already
   * sorted. A running row counts its live elapsed time, so the day total
   * ticks. */
  const groups = $derived.by(() => {
    const out: { key: string; label: string; secs: number; rows: EntryRow[] }[] =
      [];
    for (const e of timer.entries) {
      const key = String(startOfDay(e.start));
      let g = out.find((x) => x.key === key);
      if (!g) {
        g = { key, label: dayLabel(e.start), secs: 0, rows: [] };
        out.push(g);
      }
      g.rows.push(e);
      g.secs +=
        e.stop !== null
          ? e.stop - e.start
          : Math.max(0, Math.floor(now / 1000) - e.start);
    }
    return out;
  });

  async function run(fn: () => Promise<unknown>) {
    error = null;
    try {
      await fn();
    } catch (e) {
      error = String(e);
    }
  }

  const runningId = $derived(
    timer.running
      ? (timer.entries.find((e) => e.stop === null)?.id ?? null)
      : null,
  );

  function startEdit(e: EntryRow) {
    editing = e.id;
    draftDesc = e.description ?? "";
    draftTags = e.tags.join(", ");
    draftStart = e.stop === null ? "" : toTimeValue(e.start);
    draftStop = e.stop === null ? "" : toTimeValue(e.stop);
    draftProject = e.projectId;
    draftBillable = e.billable;
  }

  function cancelEdit() {
    editing = null;
  }

  async function saveEdit(e: EntryRow) {
    const start = draftStart ? todayAt(draftStart, e.start) : e.start;
    const stop =
      e.stop === null ? null : draftStop ? todayAt(draftStop, e.stop) : e.stop;
    await run(async () => {
      await invoke("update_entry", {
        id: e.id,
        description: draftDesc,
        start,
        stop,
        tags: splitTags(draftTags),
        projectId: draftProject,
        billable: draftBillable,
      });
      editing = null;
    });
  }

  async function remove(e: EntryRow) {
    await run(async () => {
      await invoke("delete_entry", { id: e.id });
      if (editing === e.id) editing = null;
    });
  }

  /** Duplicates an entry (description, project, tags) and starts it right
   * away. */
  async function duplicateAndStart(e: EntryRow) {
    await run(async () => {
      await invoke("start_timer", {
        description: e.description ?? "",
        projectId: e.projectId,
        tags: e.tags,
        billable: e.billable,
      });
    });
  }

  async function addManual() {
    const start = todayAt(manStart, Math.floor(Date.now() / 1000) - 3600);
    const stop = todayAt(manStop, Math.floor(Date.now() / 1000));
    await run(async () => {
      await invoke("create_entry", {
        description: manDesc,
        start,
        stop: stop > start ? stop : start + 60,
        tags: splitTags(manTags),
        projectId: manProject,
        billable: manBillable,
      });
      showManual = false;
      manDesc = "";
      manTags = "";
      manProject = null;
      manBillable = false;
    });
  }

  function splitTags(s: string): string[] {
    return s
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);
  }

  /** The "Name, Client" label for the project select options. */
  function projLabel(p: ProjectOption): string {
    return p.clientName ? `${p.name}, ${p.clientName}` : p.name;
  }

  const projectOptions = $derived<DropdownOption[]>([
    { value: "", label: "No project" },
    ...projects.map((p) => ({
      value: String(p.id),
      label: projLabel(p),
      color: dotColor(p.color, p.name),
    })),
  ]);

  function rowDuration(e: EntryRow): number {
    if (e.stop !== null) return Math.max(0, e.stop - e.start);
    return Math.max(0, Math.floor(now / 1000) - e.start);
  }

  /** Grows the history window. The Rust side doubles it and puts the sync
   * cursor back. */
  let loadingMore = $state(false);
  async function loadEarlier() {
    loadingMore = true;
    error = null;
    try {
      await invoke("extend_window");
    } catch (e) {
      error = String(e);
    } finally {
      loadingMore = false;
    }
  }
</script>

<div class="list-wrap">
  <div class="list">
  {#if error}
    <p class="error" style="padding:10px 16px">{error}</p>
  {/if}

  {#if showManual}
    <form
      class="manual-card"
      onsubmit={(e) => {
        e.preventDefault();
        addManual();
      }}
    >
      <input class="input" placeholder="Description" bind:value={manDesc} />
      <div class="manual-row">
        <input class="input time" type="time" bind:value={manStart} />
        <span class="muted">→</span>
        <input class="input time" type="time" bind:value={manStop} />
      </div>
      <div class="manual-row">
        <Dropdown
          options={projectOptions}
          value={manProject === null ? "" : String(manProject)}
          placeholder="No project"
          onChange={(v) => (manProject = v ? Number(v) : null)}
        />
        <label class="check">
          <input type="checkbox" bind:checked={manBillable} /> Billable
        </label>
      </div>
      <input
        class="input"
        placeholder="Tags (comma separated)"
        bind:value={manTags}
      />
      <div class="manual-row">
        <button
          class="btn primary"
          type="submit"
          disabled={!manDesc.trim()}
        >
          Save entry
        </button>
        <button class="btn ghost" type="button" onclick={() => (showManual = false)}>
          Cancel
        </button>
      </div>
    </form>
  {/if}

  {#if groups.length === 0 && !showManual}
    <div class="empty">
      Nothing tracked in the last month.
      <br />
      Start the timer above, add an entry with the + button, or load an
      earlier window below.
    </div>
  {:else}
    {#each groups as g (g.key)}
      {@const isCollapsed = collapsed.has(g.key)}
      <div class="day-head" class:collapsed={isCollapsed} role="button"
        tabindex={0}
        onclick={() => toggleGroup(g.key)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") toggleGroup(g.key);
        }}
      >
        <span class="day-title">{g.label}</span>
        <span class="day-total">{fmtDayTotal(g.secs)}</span>
        <span class="chev"><Icons name="chevron" /></span>
      </div>

      {#if !isCollapsed}
        <div class="rows">
          {#each g.rows as e (e.id)}
            {@const isRunning = e.id === runningId}
            <div class="row">
              {#if editing === e.id}
                <div class="edit-form">
                  <input
                    class="input"
                    placeholder="Description"
                    bind:value={draftDesc}
                  />
                  <div class="edit-grid">
                    {#if e.stop !== null}
                      <input class="input time" type="time" bind:value={draftStart} />
                      <span class="muted">→</span>
                      <input class="input time" type="time" bind:value={draftStop} />
                    {/if}
                    <input
                      class="input"
                      placeholder="Tags (comma separated)"
                      bind:value={draftTags}
                    />
                    <Dropdown
                      options={projectOptions}
                      value={draftProject === null ? "" : String(draftProject)}
                      placeholder="No project"
                      onChange={(v) => (draftProject = v ? Number(v) : null)}
                    />
                    <label class="check">
                      <input type="checkbox" bind:checked={draftBillable} />
                      Billable
                    </label>
                  </div>
                  <div class="edit-actions">
                    <button class="btn primary" onclick={() => saveEdit(e)}>
                      Save
                    </button>
                    <button class="btn ghost" onclick={cancelEdit}>Cancel</button>
                  </div>
                </div>
              {:else}
                <div class="row-body">
                  <span class="row-desc" class:untitled={!e.description}>
                    {e.description ?? "Untitled"}
                  </span>
                  <div class="row-sub">
                    {#if e.projectName}
                      <span
                        class="row-project"
                        style="color:{dotColor(e.projectColor, e.projectName)}"
                      >
                        <span
                          class="dot"
                          style="background:{dotColor(e.projectColor, e.projectName)}"
                        ></span>
                        {e.projectName}
                      </span>
                    {/if}
                    {#if e.clientName}
                      <span class="row-client">{e.clientName}</span>
                    {/if}
                    {#each e.tags as t (t)}
                      <span class="row-tag">{t}</span>
                    {/each}
                  </div>
                </div>

                <div class="row-actions">
                  {#if !isRunning && !timer.running}
                    <button
                      class="icon-round"
                      title="Continue time entry"
                      aria-label="Continue time entry"
                      onclick={() => duplicateAndStart(e)}
                    >
                      <Icons name="play" />
                    </button>
                  {/if}
                  {#if !isRunning}
                    <button
                      class="icon-round"
                      title="Edit"
                      aria-label="Edit entry"
                      onclick={() => startEdit(e)}
                    >
                      <Icons name="pencil" />
                    </button>
                  {/if}
                  <button
                    class="icon-round"
                    title="Delete"
                    aria-label="Delete entry"
                    onclick={() => remove(e)}
                  >
                    <Icons name="trash" />
                  </button>
                </div>

                {#if e.dirty}
                  <span class="dirty-dot" title="Not synced yet">●</span>
                {/if}

                {#if e.billable}
                  <span class="row-billable" title="Billable">
                    <Icons name="dollar" />
                  </span>
                {/if}

                {#if isRunning}
                  <span class="row-dur running">{fmtDuration(rowDuration(e))}</span>
                  <button
                    class="row-stop"
                    title="Stop timer"
                    aria-label="Stop timer"
                    onclick={() => invoke("stop_timer").catch((err) => (error = String(err)))}
                  >
                    <i></i>
                  </button>
                {:else}
                  <span class="row-time">{fmtClock(e.start)}</span>
                  <span class="row-dur">{fmtDuration(rowDuration(e))}</span>
                {/if}
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/each}
  {/if}

  <button class="load-earlier" disabled={loadingMore} onclick={loadEarlier}>
    {loadingMore ? "Loading…" : "Load earlier entries"}
  </button>
  </div>

  <button
    class="fab"
    title="Add manual entry"
    aria-label="Add manual entry"
    onclick={() => (showManual = !showManual)}
  >
    {showManual ? "×" : "+"}
  </button>
</div>
