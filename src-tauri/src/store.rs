//! SQLite cache for time entries and sync bookkeeping.
//!
//! Design for the budget of 30 requests per hour on the free plan:
//!   * The UI always renders from this cache. That is instant and uses no
//!     requests after the last sync.
//!   * Edits are written through at once. The sync loop re-applies any row
//!     with `dirty = 1`. Each one costs one API request, oldest first, as the
//!     budget allows.
//!   * Delta sync uses `since = <the largest last_synced seen so far>`, so
//!     repeated polls transfer (and cost) as little as possible.
//!   * Deletes are marked locally with `deleted = 1`. The row can still be
//!     found as dirty for the pending DELETE call.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::toggl::{Client, TimeEntry, WorkspaceProject, WorkspaceTag};

/// A cached entry as the UI shows it. Rows marked `deleted` are filtered out.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EntryRow {
    pub id: i64,
    pub workspace_id: i64,
    pub description: Option<String>,
    /// Unix seconds.
    pub start: i64,
    /// Unix seconds. None while the entry is running.
    pub stop: Option<i64>,
    pub duration: f64,
    pub project_id: Option<i64>,
    /// From the cached project list, when known.
    pub project_name: Option<String>,
    /// The Toggl color for the project, as hex with no `#`. The UI falls back
    /// to a deterministic color when this is absent.
    pub project_color: Option<String>,
    /// The client that owns the project, when the project has one.
    pub client_name: Option<String>,
    pub tags: Vec<String>,
    pub billable: bool,
    pub dirty: bool,
}

/// A cached row that is flagged for re-application to the server.
pub struct DirtyEntry {
    pub id: i64,
    pub workspace_id: i64,
    pub description: Option<String>,
    pub start: i64,
    pub stop: Option<i64>,
    /// The raw cached duration. A push recomputes an integer duration from
    /// start and stop, because v9 rejects floats. This read-back value is
    /// unused.
    #[allow(dead_code)]
    pub duration: f64,
    pub project_id: Option<i64>,
    pub tags: Vec<String>,
    pub billable: bool,
    pub deleted: bool,
}

/// A project option for the pickers and row labels. Cached from
/// `GET /workspaces/{id}/projects` and joined with the client list.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PickerProject {
    pub id: i64,
    pub name: String,
    pub billable: bool,
    /// The Toggl hex color (no `#`), when known.
    pub color: Option<String>,
    /// The name of the owning client, when the project has a client.
    pub client_name: Option<String>,
}

pub struct Store {
    pub conn: Mutex<Connection>,
}

impl Store {
    /// Opens the cache DB under `data_dir` (the app data dir) and applies the
    /// schema. Returns None on any failure. The app then runs on live API
    /// data only, and the Today list simply stays empty.
    pub fn open(data_dir: &Path) -> Option<Store> {
        let dir = data_dir.join("cache");
        std::fs::create_dir_all(&dir).ok()?;
        let conn = Connection::open(dir.join("trackfecta.db")).ok()?;
        conn.pragma_update(None, "journal_mode", "WAL").ok()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS time_entries (
                id             INTEGER PRIMARY KEY,
                workspace_id   INTEGER NOT NULL,
                description    TEXT,
                start_ts       INTEGER NOT NULL,
                stop_ts        INTEGER,
                duration       REAL NOT NULL DEFAULT 0,
                project_id     INTEGER,
                tags           TEXT NOT NULL DEFAULT '',
                updated_ts     INTEGER NOT NULL DEFAULT 0,
                deleted        INTEGER NOT NULL DEFAULT 0,
                dirty          INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS sync_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS picker_projects (
                id         INTEGER PRIMARY KEY,
                name       TEXT NOT NULL,
                billable   INTEGER NOT NULL DEFAULT 0,
                color      TEXT,
                client_id  INTEGER,
                recent     INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS picker_tags (
                name TEXT PRIMARY KEY
            );
            CREATE TABLE IF NOT EXISTS clients (
                id   INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .ok()?;
        // Add columns for caches created before those columns existed.
        let _ = conn.execute(
            "ALTER TABLE time_entries ADD COLUMN billable INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE picker_projects ADD COLUMN color TEXT", []);
        let _ = conn.execute(
            "ALTER TABLE picker_projects ADD COLUMN client_id INTEGER",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE picker_projects ADD COLUMN recent INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Some(Store {
            conn: Mutex::new(conn),
        })
    }

    pub fn get_meta(&self, key: &str) -> Option<i64> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT value FROM sync_meta WHERE key = ?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .ok()?
        .parse()
        .ok()
    }

    pub fn set_meta(&self, key: &str, value: i64) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO sync_meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, value.to_string()],
            );
        }
    }

    /// A user preference, stored as a string: hourly cap, default project,
    /// and so on.
    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO settings(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, value],
            );
        }
    }

    pub fn clear_setting(&self, key: &str) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute("DELETE FROM settings WHERE key = ?1", params![key]);
        }
    }

    /// Stores the full `GET /workspaces/{id}/projects` payload in the cached
    /// project table. The `recent` flags stay as they are.
    pub fn replace_projects(&self, projects: &[WorkspaceProject]) {
        let Ok(conn) = self.conn.lock() else { return };
        for p in projects {
            let _ = conn.execute(
                "INSERT INTO picker_projects(id, name, billable, color, client_id, recent)
                 VALUES(?1,?2,?3,?4,?5,0)
                 ON CONFLICT(id) DO UPDATE SET
                    name=excluded.name,
                    billable=excluded.billable,
                    color=COALESCE(excluded.color, picker_projects.color),
                    client_id=COALESCE(excluded.client_id, picker_projects.client_id)",
                params![
                    p.id,
                    p.name,
                    p.billable.unwrap_or(false) as i64,
                    p.color,
                    p.client_id
                ],
            );
        }
    }

    /// Replaces the cached client names (from `GET /workspaces/{id}/clients`).
    pub fn replace_clients(&self, clients: &[Client]) {
        let Ok(conn) = self.conn.lock() else { return };
        let _ = conn.execute("DELETE FROM clients", []);
        for c in clients {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO clients(id, name) VALUES(?1,?2)",
                params![c.id, c.name],
            );
        }
    }

    /// Replaces the cached tag list. Source: `GET /workspaces/{id}/tags`.
    /// This is the tag-picker source since `/me/interests` was removed
    /// upstream.
    pub fn replace_tags(&self, tags: &[WorkspaceTag]) {
        let Ok(conn) = self.conn.lock() else { return };
        let _ = conn.execute("DELETE FROM picker_tags", []);
        for t in tags {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO picker_tags(name) VALUES(?1)",
                params![t.name],
            );
        }
    }

    /// The cached project ids that are not in the given set. Sync uses this
    /// to drop projects that were deleted on the server.
    pub fn project_ids_except(&self, keep: &[i64]) -> Vec<i64> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare("SELECT id FROM picker_projects") else {
            return Vec::new();
        };
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .map(|it| {
                it.filter_map(Result::ok)
                    .filter(|id| !keep.contains(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Removes cached projects by id (server-side deletions).
    pub fn remove_projects(&self, ids: &[i64]) {
        let Ok(conn) = self.conn.lock() else { return };
        for id in ids {
            let _ = conn.execute("DELETE FROM picker_projects WHERE id=?1", params![id]);
        }
    }

    pub fn picker_projects(&self) -> Vec<PickerProject> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        // "Recent first" means the projects that cached entries actually
        // reference, newest usage first. The rest follow alphabetically.
        let Ok(mut stmt) = conn.prepare(
            "SELECT pp.id, pp.name, pp.billable, pp.color, c.name
             FROM picker_projects pp
             LEFT JOIN clients c ON c.id = pp.client_id
             ORDER BY (SELECT MAX(te.start_ts) FROM time_entries te
                        WHERE te.project_id = pp.id AND te.deleted = 0) DESC,
                      pp.recent DESC,
                      pp.name COLLATE NOCASE",
        ) else {
            return Vec::new();
        };
        let rows = stmt.query_map([], |r| {
            Ok(PickerProject {
                id: r.get(0)?,
                name: r.get(1)?,
                billable: r.get::<_, i64>(2)? != 0,
                color: r.get(3)?,
                client_name: r.get(4)?,
            })
        });
        rows.map(|it| it.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    pub fn picker_tags(&self) -> Vec<String> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) =
            conn.prepare("SELECT name FROM picker_tags ORDER BY name COLLATE NOCASE")
        else {
            return Vec::new();
        };
        let rows = stmt.query_map([], |r| r.get::<_, String>(0));
        rows.map(|it| it.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// Stores a server entry. The cache never overwrites a row that has a
    /// pending local change (`dirty`). The server copy is older than the
    /// queued push. Overwriting such rows resurrected locally-deleted entries
    /// (see the ON CONFLICT guard below) while their DELETE was still
    /// queued.
    pub fn upsert(&self, e: &TimeEntry) {
        let Ok(conn) = self.conn.lock() else { return };
        let tags = e.tags.clone().unwrap_or_default().join(",");
        let deleted = e.deleted.unwrap_or(false) as i64;
        let billable = e.billable.unwrap_or(false) as i64;
        let updated_ts = e
            .updated_at
            .map(|t| t.timestamp())
            .unwrap_or_else(|| chrono::Utc::now().timestamp());
        let _ = conn.execute(
            "INSERT INTO time_entries
                (id, workspace_id, description, start_ts, stop_ts, duration,
                 project_id, tags, updated_ts, deleted, dirty, billable)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,?11)
             ON CONFLICT(id) DO UPDATE SET
                workspace_id=excluded.workspace_id,
                description=excluded.description,
                start_ts=excluded.start_ts,
                stop_ts=excluded.stop_ts,
                duration=excluded.duration,
                project_id=excluded.project_id,
                tags=excluded.tags,
                updated_ts=excluded.updated_ts,
                deleted=excluded.deleted,
                billable=excluded.billable
             WHERE time_entries.dirty = 0",
            params![
                e.id,
                e.workspace_id,
                e.description,
                e.start.timestamp(),
                e.stop.map(|s| s.timestamp()),
                e.duration,
                e.project_id,
                tags,
                updated_ts,
                deleted,
                billable
            ],
        );
    }

    /// Marks a cached row as deleted from a delta-sync deletion record.
    /// Those records are partial (`{id, at, deleted:true}`), so they must
    /// never go through `upsert`. That would blank the start and stop of the
    /// row. Ids that were never cached are ignored. There is nothing to
    /// remove locally.
    pub fn apply_tombstone(&self, id: i64) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "UPDATE time_entries SET deleted=1, dirty=0 WHERE id=?1",
                params![id],
            );
        }
    }

    /// Physically removes rows confirmed as deleted: server-side deletions
    /// and local deletes whose DELETE call landed. This keeps the cache from
    /// growing. Pending pushes (`deleted=1 AND dirty=1`) are preserved.
    pub fn purge_tombstones(&self) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute("DELETE FROM time_entries WHERE deleted=1 AND dirty=0", []);
        }
    }

    /// Marks cached rows of `workspace_id` with a start at or after `from_ts`
    /// as deleted when a full server fetch does not include them
    /// (`present_ids`). Those were deleted on the web or another device while
    /// this app was not polling. Rows with a pending local change (`dirty`)
    /// are never touched. Returns how many rows were removed.
    pub fn reconcile_absent(&self, workspace_id: i64, from_ts: i64, present_ids: &[i64]) -> i64 {
        let Ok(conn) = self.conn.lock() else { return 0 };
        let mut stmt = match conn.prepare(
            "SELECT id FROM time_entries
             WHERE dirty=0 AND deleted=0 AND workspace_id=?1 AND start_ts>=?2",
        ) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let stale: Vec<i64> = stmt
            .query_map(params![workspace_id, from_ts], |r| r.get::<_, i64>(0))
            .map(|it| {
                it.filter_map(Result::ok)
                    .filter(|id| !present_ids.contains(id))
                    .collect()
            })
            .unwrap_or_default();
        drop(stmt);
        let mut n = 0;
        for id in &stale {
            let _ = conn.execute(
                "UPDATE time_entries SET deleted=1, dirty=0 WHERE id=?1",
                params![id],
            );
            n += 1;
        }
        n
    }

    /// Inserts a manual entry the user created, flagged dirty (a POST is
    /// pending).
    pub fn insert_manual(&self, e: &TimeEntry) {
        let Ok(conn) = self.conn.lock() else { return };
        let tags = e.tags.clone().unwrap_or_default().join(",");
        let billable = e.billable.unwrap_or(false) as i64;
        let _ = conn.execute(
            "INSERT OR REPLACE INTO time_entries
                (id, workspace_id, description, start_ts, stop_ts, duration,
                 project_id, tags, updated_ts, deleted, dirty, billable)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,1,?10)",
            params![
                e.id,
                e.workspace_id,
                e.description,
                e.start.timestamp(),
                e.stop.map(|s| s.timestamp()),
                e.duration,
                e.project_id,
                tags,
                chrono::Utc::now().timestamp(),
                billable
            ],
        );
    }

    /// Local write-through for edits and deletes. This keeps the UI instant.
    // The fields mirror a time entry. Bundling them into a struct would only
    // add a type used once, so the flat form stays.
    #[allow(clippy::too_many_arguments)]
    pub fn update_local(
        &self,
        id: i64,
        description: Option<&str>,
        tags: &[String],
        start: i64,
        stop: Option<i64>,
        project_id: Option<i64>,
        billable: bool,
    ) {
        let Ok(conn) = self.conn.lock() else { return };
        let duration = match stop {
            Some(s) => (s - start) as f64,
            // The running convention. Not used for duration display.
            None => -start as f64,
        };
        let _ = conn.execute(
            "UPDATE time_entries
             SET description=?2, tags=?3, start_ts=?4, stop_ts=?5, duration=?6,
                 project_id=?7, billable=?8, dirty=1
             WHERE id=?1",
            params![
                id,
                description,
                tags.join(","),
                start,
                stop,
                duration,
                project_id,
                billable as i64
            ],
        );
    }

    /// Stops a running row in place: sets stop and duration, keeps the
    /// description and tags, and marks the row dirty for the queued PUT.
    pub fn stop_local(&self, id: i64, stop_ts: i64) {
        let Ok(conn) = self.conn.lock() else { return };
        let _ = conn.execute(
            "UPDATE time_entries
             SET stop_ts=?2, duration=MAX(0, ?2 - start_ts), dirty=1
             WHERE id=?1",
            params![id, stop_ts],
        );
    }

    pub fn mark_deleted(&self, id: i64) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "UPDATE time_entries SET deleted=1, dirty=1 WHERE id=?1",
                params![id],
            );
        }
    }

    /// Clears the dirty flag after a successful push. `None` keeps the stored
    /// deleted value. Used after a DELETE, where the row becomes a deletion
    /// record.
    pub fn clear_dirty(&self, id: i64, deleted: Option<bool>) {
        if let Ok(conn) = self.conn.lock() {
            match deleted {
                Some(d) => {
                    let _ = conn.execute(
                        "UPDATE time_entries SET dirty=0, deleted=?2 WHERE id=?1",
                        params![id, d as i64],
                    );
                }
                None => {
                    let _ =
                        conn.execute("UPDATE time_entries SET dirty=0 WHERE id=?1", params![id]);
                }
            }
        }
    }

    /// Removes local rows that never made it to the server: negative-id
    /// manual entries whose push failed. This stops the list from showing
    /// phantom rows.
    pub fn remove_if_unsynced(&self, id: i64) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "DELETE FROM time_entries WHERE id=?1 AND dirty=1",
                params![id],
            );
        }
    }

    pub fn dirty_entries(&self) -> Vec<DirtyEntry> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            "SELECT id, workspace_id, description, start_ts, stop_ts, duration,
                    project_id, tags, deleted, billable
             FROM time_entries WHERE dirty=1 ORDER BY updated_ts ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |r| {
            let tags: String = r.get(7)?;
            Ok(DirtyEntry {
                id: r.get(0)?,
                workspace_id: r.get(1)?,
                description: r.get(2)?,
                start: r.get(3)?,
                stop: r.get(4)?,
                duration: r.get(5)?,
                project_id: r.get(6)?,
                tags: if tags.is_empty() {
                    Vec::new()
                } else {
                    tags.split(',').map(|s| s.to_string()).collect()
                },
                deleted: r.get::<_, i64>(8)? != 0,
                billable: r.get::<_, i64>(9)? != 0,
            })
        });
        rows.map(|it| it.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// All cached entries with a start at or after `from_ts`, without the
    /// deleted rows, newest first. Joined against the cached picker list, so
    /// each row carries its project name for the colored label.
    pub fn entries_since(&self, from_ts: i64) -> Vec<EntryRow> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            "SELECT e.id, e.workspace_id, e.description, e.start_ts, e.stop_ts, e.duration,
                    e.project_id, e.tags, e.dirty, e.billable, p.name, p.color, c.name
             FROM time_entries e
             LEFT JOIN picker_projects p ON p.id = e.project_id
             LEFT JOIN clients c ON c.id = p.client_id
             WHERE e.deleted=0 AND e.start_ts >= ?1
             ORDER BY e.start_ts DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![from_ts], read_entry_row);
        rows.map(|it| it.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// The open (running) entry from the cache, when there is one. The cache
    /// is the authority for "is something running". A delta poll does not
    /// re-send the running entry once its `updated_at` is behind the cursor.
    pub fn open_entry(&self) -> Option<EntryRow> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT e.id, e.workspace_id, e.description, e.start_ts, e.stop_ts, e.duration,
                    e.project_id, e.tags, e.dirty, e.billable, p.name, p.color, c.name
             FROM time_entries e
             LEFT JOIN picker_projects p ON p.id = e.project_id
             LEFT JOIN clients c ON c.id = p.client_id
             WHERE e.deleted=0 AND e.stop_ts IS NULL
             ORDER BY e.start_ts DESC LIMIT 1",
            [],
            read_entry_row,
        )
        .ok()
    }
}

/// Shared row reader for the entry queries. The column order must match
/// their SELECT lists.
fn read_entry_row(r: &rusqlite::Row) -> rusqlite::Result<EntryRow> {
    let tags: String = r.get(7)?;
    Ok(EntryRow {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        description: r.get(2)?,
        start: r.get(3)?,
        stop: r.get(4)?,
        duration: r.get(5)?,
        project_id: r.get(6)?,
        tags: if tags.is_empty() {
            Vec::new()
        } else {
            tags.split(',').map(|s| s.to_string()).collect()
        },
        dirty: r.get::<_, i64>(8)? != 0,
        billable: r.get::<_, i64>(9)? != 0,
        project_name: r.get(10)?,
        project_color: r.get(11)?,
        client_name: r.get(12)?,
    })
}
