<script lang="ts">
  let {
    name,
    size = 16,
  }: { name: string; size?: number } = $props();
</script>

{#if name === "play"}
  <svg viewBox="0 0 16 16" width={size} height={size} fill="currentColor"
    ><path d="M4.5 2.8c0-.7.75-1.15 1.36-.8l8.1 5.2c.57.36.57 1.23 0 1.6l-8.1 5.2c-.6.38-1.36-.1-1.36-.8V2.8Z"
  /></svg>
{:else if name === "stop"}
  <svg viewBox="0 0 16 16" width={size} height={size} fill="currentColor"
    ><rect x="2.5" y="2.5" width="11" height="11" rx="2" /></svg
>
{:else if name === "folder"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linejoin="round"
    ><path d="M1.8 4.2c0-.7.55-1.2 1.2-1.2h2.6c.5 0 .95.28 1.17.72l.5.98h4.53c.66 0 1.2.55 1.2 1.2v5.9c0 .66-.54 1.2-1.2 1.2H3c-.65 0-1.2-.54-1.2-1.2V4.2Z"
  /></svg>
{:else if name === "tag"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linejoin="round"
    ><path
      d="M7.3 1.9H13a1.1 1.1 0 0 1 1.1 1.1v5.7a1.1 1.1 0 0 1-.32.78l-5.9 5.9a1.1 1.1 0 0 1-1.56 0L1.9 11.5a1.1 1.1 0 0 1 0-1.56l5.4-5.4a1.1 1.1 0 0 1 .78-.32l-1.56 0"
    /><circle cx="11" cy="5" r="1" fill="currentColor" stroke="none" /></svg>
{:else if name === "dollar"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linecap="round"
    ><path d="M8 1.6v12.8" /><path
      d="M10.8 4.4c-.6-.9-1.6-1.3-2.9-1.3-1.7 0-2.9.9-2.9 2.3 0 1.5 1.2 2 3 2.5 2 .5 3.1 1.1 3.1 2.6 0 1.5-1.3 2.4-3.1 2.4-1.5 0-2.6-.6-3.1-1.6"
    /></svg>
{:else if name === "gear"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="currentColor"
    fill-rule="evenodd"
    ><path
      d="M6.63 2.88L6.91 0.73L9.09 0.73L9.37 2.88A5.3 5.3 0 0 1 10.65 3.41L12.37 2.09L13.91 3.63L12.59 5.35A5.3 5.3 0 0 1 13.12 6.63L15.27 6.91L15.27 9.09L13.12 9.37A5.3 5.3 0 0 1 12.59 10.65L13.91 12.37L12.37 13.91L10.65 12.59A5.3 5.3 0 0 1 9.37 13.12L9.09 15.27L6.91 15.27L6.63 13.12A5.3 5.3 0 0 1 5.35 12.59L3.63 13.91L2.09 12.37L3.41 10.65A5.3 5.3 0 0 1 2.88 9.37L0.73 9.09L0.73 6.91L2.88 6.63A5.3 5.3 0 0 1 3.41 5.35L2.09 3.63L3.63 2.09L5.35 3.41A5.3 5.3 0 0 1 6.63 2.88ZM5.55 8A2.45 2.45 0 1 0 10.45 8A2.45 2.45 0 1 0 5.55 8Z"
    /></svg>
{:else if name === "chevron"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.6"
    stroke-linecap="round"
    stroke-linejoin="round"
    ><path d="M4 10l4-4 4 4" /></svg
>
{:else if name === "pencil"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linejoin="round"
    ><path d="M11.2 2.3 13.7 4.8 5.5 13H3v-2.5l8.2-8.2Z" /></svg
>
{:else if name === "trash"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linecap="round"
    stroke-linejoin="round"
    ><path d="M2.8 4.2h10.4M6 4.2V2.8h4v1.4M4.2 4.2l.6 9h6.4l.6-9M6.8 7v3.6M9.2 7v3.6" /></svg
>
{:else if name === "copy"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linejoin="round"
    ><rect x="5.4" y="5.4" width="8.2" height="8.2" rx="1.4" /><path
      d="M10.6 5.4V3.8c0-.7-.6-1.2-1.2-1.2H3.8c-.7 0-1.2.5-1.2 1.2v5.6c0 .65.5 1.2 1.2 1.2h1.6"
    /></svg
>
{:else if name === "check"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    ><path d="M3 8.5 6.5 12 13 4.5" /></svg
>
{:else if name === "sync"}
  <!-- Feather "rotate-cw", scaled from a 24 viewBox to 16. -->
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.5"
    stroke-linecap="round"
    stroke-linejoin="round"
    ><path d="M15.3 2.7v4h-4" /><path
      d="M13.7 10a6 6 0 1 1-1.4-6.2l3 2.9"
    /></svg
>
{:else if name === "list"}
  <svg
    viewBox="0 0 16 16"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    stroke-width="1.5"
    stroke-linecap="round"
    ><path d="M5.5 3.5h8M5.5 8h8M5.5 12.5h8" /><circle
      cx="2.6"
      cy="3.5"
      r="0.9"
      fill="currentColor"
      stroke="none"
    /><circle
      cx="2.6"
      cy="8"
      r="0.9"
      fill="currentColor"
      stroke="none"
    /><circle
      cx="2.6"
      cy="12.5"
      r="0.9"
      fill="currentColor"
      stroke="none"
    /></svg
>
{/if}
