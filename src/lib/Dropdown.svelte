<script lang="ts">
  import Icons from "./Icons.svelte";

  /** One dropdown option. The value "" is the sentinel for "none" or
   * "clear". */
  export type DropdownOption = {
    value: string;
    label: string;
    /** The dot color for project-style options. */
    color?: string | null;
  };

  let {
    options,
    value,
    placeholder = "Select…",
    icon = "folder",
    onChange,
  }: {
    options: DropdownOption[];
    value: string;
    placeholder?: string;
    /** The leading icon name (folder, tag, and so on). "" means none. */
    icon?: string;
    onChange: (v: string) => void;
  } = $props();

  let open = $state(false);
  let btn: HTMLButtonElement | undefined = $state();
  let popStyle = $state("");
  const selected = $derived(options.find((o) => o.value === value) ?? null);

  /** Fixed positioning. The dropdowns are inside scrolling lists. An
   * absolutely-positioned popover there would be cut off by the scroller. */
  function toggle() {
    open = !open;
    if (!open || !btn) return;
    const r = btn.getBoundingClientRect();
    const roomBelow = window.innerHeight - r.bottom;
    const minW = Math.max(r.width, 200);
    if (roomBelow > 260 || roomBelow >= r.top) {
      popStyle = `left:${r.left}px;top:${r.bottom + 4}px;min-width:${minW}px;max-height:${Math.min(300, roomBelow - 12)}px`;
    } else {
      popStyle = `left:${r.left}px;bottom:${window.innerHeight - r.top + 4}px;min-width:${minW}px;max-height:${Math.min(300, r.top - 12)}px`;
    }
  }

  function pick(v: string) {
    open = false;
    if (v !== value) onChange(v);
  }
</script>

<div class="pop-anchor">
  <button
    bind:this={btn}
    class="chip-btn dd-btn"
    class:open
    class:selected={selected && selected.value !== ""}
    type="button"
    onclick={toggle}
  >
    {#if icon}<Icons name={icon} />{/if}
    {#if selected && selected.value !== ""}
      {#if selected.color}
        <span class="dot" style="background:{selected.color}"></span>
      {/if}
      <span class="label">{selected.label}</span>
    {:else}
      <span class="label muted-label">{placeholder}</span>
    {/if}
    <span class="dd-caret"><Icons name="chevron" size={12} /></span>
  </button>
  {#if open}
    <div class="pop dd-pop" style={popStyle}>
      <div class="pop-list">
        {#each options as o (o.value)}
          <button class="pop-item" type="button" onclick={() => pick(o.value)}>
            {#if o.color}
              <span class="dot" style="background:{o.color}"></span>
            {/if}
            <span class="name">{o.label}</span>
            {#if o.value === value}<span class="pop-check">✓</span>{/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

{#if open}
  <button
    class="scrim"
    aria-label="Close menu"
    tabindex="-1"
    onclick={() => (open = false)}></button>
{/if}
