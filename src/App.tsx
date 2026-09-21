import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type TimerState = {
  running: boolean;
  description: string | null;
  startedAt: string | null;
  todaySeconds: number;
};

const EMPTY: TimerState = {
  running: false,
  description: null,
  startedAt: null,
  todaySeconds: 0,
};

function fmtDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export default function App() {
  const [hasToken, setHasToken] = useState<boolean | null>(null);
  const [token, setToken] = useState("");
  const [state, setState] = useState<TimerState>(EMPTY);
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    invoke<boolean>("has_api_token")
      .then(setHasToken)
      .catch(() => setHasToken(false));

    const un = listen<TimerState>("timer-state", (e) => setState(e.payload));
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      un.then((f) => f());
      clearInterval(tick);
    };
  }, []);

  async function saveToken() {
    await invoke("set_api_token", { token: token.trim() });
    setHasToken(true);
  }

  if (hasToken === null) return <Splash />;

  if (!hasToken) {
    return (
      <main className="shell">
        <section className="card auth-card">
          <h1>Welcome to ToggLinux</h1>
          <p className="muted">
            Paste your Toggl&nbsp;Track API token to get started. You can find
            it on your{" "}
            <a href="https://track.toggl.com/profile" target="_blank" rel="noreferrer">
              profile page
            </a>
            . It is stored securely in your system keyring.
          </p>
          <input
            className="input"
            type="password"
            placeholder="API token"
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
          <button className="btn primary" disabled={!token.trim()} onClick={saveToken}>
            Connect
          </button>
          <p className="fineprint">
            ToggLinux is an unofficial client and is not affiliated with,
            endorsed by, or sponsored by Toggl™.
          </p>
        </section>
      </main>
    );
  }

  const elapsed =
    state.running && state.startedAt
      ? state.todaySeconds + Math.max(0, Math.floor((now - Number(state.startedAt) * 1000) / 1000))
      : state.todaySeconds;

  return (
    <main className="shell">
      <header className="topbar">
        <span className="brand">ToggLinux</span>
        <span className="muted">unofficial Toggl Track client</span>
      </header>

      <section className="card timer-card">
        {state.running ? (
          <>
            <div className="desc">{state.description ?? "Untitled"}</div>
            <div className="clock running">{fmtDuration(elapsed)}</div>
            <button
              className="btn stop"
              onClick={() => invoke("stop_timer")}
            >
              ■ Stop
            </button>
          </>
        ) : (
          <>
            <div className="desc muted">No timer running</div>
            <div className="clock">{fmtDuration(state.todaySeconds)} today</div>
            <button
              className="btn primary"
              onClick={() => invoke("start_timer", { description: "Quick entry" })}
            >
              ▶ Start
            </button>
          </>
        )}
      </section>
    </main>
  );
}

function Splash() {
  return (
    <main className="shell">
      <section className="card">
        <p className="muted">Loading…</p>
      </section>
    </main>
  );
}
