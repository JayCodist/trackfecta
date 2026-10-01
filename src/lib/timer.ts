// Shared timer types + helpers (mirrors the Rust `TimerState` in src-tauri/src/state.rs)

export type ConnStatus = "verifying" | "disconnected" | "connected";

export type TimerState = {
  running: boolean;
  description: string | null;
  /** Unix seconds, as a string. */
  startedAt: string | null;
  /** Seconds tracked today, excluding the running entry. */
  todaySeconds: number;
  status: ConnStatus;
};

export const EMPTY: TimerState = {
  running: false,
  description: null,
  startedAt: null,
  todaySeconds: 0,
  status: "verifying",
};

export function fmtDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function fmtShort(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? `${h}h ${m}m` : `${m}m`;
}
