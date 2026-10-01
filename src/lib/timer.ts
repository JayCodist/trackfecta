// Shared timer types + helpers (mirrors the Rust `TimerState` in src-tauri/src/lib.rs)

export type TimerState = {
  running: boolean;
  description: string | null;
  startedAt: string | null;
  todaySeconds: number;
};

export const EMPTY: TimerState = {
  running: false,
  description: null,
  startedAt: null,
  todaySeconds: 0,
};

export function fmtDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}
