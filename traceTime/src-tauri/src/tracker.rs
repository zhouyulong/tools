use crate::reminder::{
    DueReminder, OVERDUE_INTERVAL_KEY, ReminderCheckDto, ReminderConfig, ReminderConfigDto,
    ReminderLevel, due_reminders, parse_overdue_interval,
};
use crate::store::OVERDUE_UPGRADE_CUTOFF_KEY;
use chrono::{DateTime, FixedOffset, TimeDelta, Utc};
use rusqlite::{Connection, Error as SqlError};
use serde::Serialize;
use std::sync::Mutex;

const PAUSE_REASONS: [&str; 4] = ["rest", "disturbance", "switch", "other"];
const TASK_COLUMNS: &str = "id, name, est_seconds, status, tag, COALESCE(net_seconds, 0) AS net_seconds, created_at, finished_at, reminder_muted";

#[derive(Debug, Serialize, Clone)]
pub struct TaskDto {
    pub id: String,
    pub name: String,
    pub est_seconds: i64,
    pub status: String,
    pub tag: Option<String>,
    pub net_seconds: i64,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub paused_since: Option<String>,
    pub reminder_muted: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct SegmentDto {
    pub started_at: String,
    pub ended_at: String,
    pub kind: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct EventDto {
    pub id: i64,
    pub at: String,
    pub kind: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TaskDetailDto {
    pub task: TaskDto,
    pub segments: Vec<SegmentDto>,
    pub events: Vec<EventDto>,
}

pub struct Tracker {
    pub(crate) conn: Mutex<Connection>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<TaskDto> {
    Ok(TaskDto {
        id: r.get("id")?,
        name: r.get("name")?,
        est_seconds: r.get("est_seconds")?,
        status: r.get("status")?,
        tag: r.get("tag")?,
        net_seconds: r.get("net_seconds")?,
        created_at: r.get("created_at")?,
        finished_at: r.get("finished_at")?,
        paused_since: None,
        reminder_muted: r.get::<_, i64>("reminder_muted")? != 0,
    })
}

impl Tracker {
    pub fn new(conn: Connection) -> Tracker {
        Tracker {
            conn: Mutex::new(conn),
        }
    }

    fn add_event(
        conn: &Connection,
        task_id: &str,
        kind: &str,
        detail: Option<&str>,
    ) -> Result<(), String> {
        conn.execute(
            "INSERT INTO events (task_id, at, kind, detail) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![task_id, now(), kind, detail],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    fn parse_timestamp(at: &str) -> Result<DateTime<FixedOffset>, String> {
        DateTime::parse_from_rfc3339(at).map_err(|e| e.to_string())
    }

    fn elapsed(start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> TimeDelta {
        (end - start).max(TimeDelta::zero())
    }

    fn visit_closed_work(
        conn: &Connection,
        task_id: &str,
        mut visit: impl FnMut(DateTime<FixedOffset>, DateTime<FixedOffset>),
    ) -> Result<(), String> {
        let mut stmt = conn
            .prepare(
                "SELECT started_at, ended_at FROM segments
                 WHERE task_id = ?1 AND kind = 'work' AND ended_at IS NOT NULL ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([task_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (start, end) = row.map_err(|e| e.to_string())?;
            visit(Self::parse_timestamp(&start)?, Self::parse_timestamp(&end)?);
        }
        Ok(())
    }

    fn closed_work_duration(conn: &Connection, task_id: &str) -> Result<TimeDelta, String> {
        let mut total = TimeDelta::zero();
        Self::visit_closed_work(conn, task_id, |start, end| {
            total += Self::elapsed(start, end)
        })?;
        Ok(total)
    }

    fn closed_net_seconds(conn: &Connection, task_id: &str) -> Result<i64, String> {
        // Retain subsecond precision across all segments; truncate only at the seconds API boundary.
        Ok(Self::closed_work_duration(conn, task_id)?.num_seconds())
    }

    // Event ids preserve append order even when wall-clock timestamps move backwards.
    fn latest_start_at(conn: &Connection, task_id: &str) -> Result<Option<String>, String> {
        match conn.query_row(
            "SELECT at FROM events
             WHERE task_id = ?1 AND kind IN ('started', 'resumed')
             ORDER BY id DESC LIMIT 1",
            [task_id],
            |r| r.get(0),
        ) {
            Ok(at) => Ok(Some(at)),
            Err(SqlError::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn running_start(conn: &Connection, task: &TaskDto) -> Result<DateTime<FixedOffset>, String> {
        let at = Self::latest_start_at(conn, &task.id)?
            .ok_or_else(|| "running task has no start event".to_string())?;
        Self::parse_timestamp(&at)
    }

    fn net_seconds(conn: &Connection, task: &TaskDto) -> Result<i64, String> {
        let closed = Self::closed_work_duration(conn, &task.id)?;
        if task.status != "running" {
            return Ok(closed.num_seconds());
        }
        let live = Self::elapsed(Self::running_start(conn, task)?, Utc::now().fixed_offset());
        // The live segment participates in the same sum, without per-segment rounding.
        Ok((closed + live).num_seconds())
    }

    fn gross_seconds(conn: &Connection, task_id: &str, ended_at: &str) -> Result<i64, String> {
        let created_at: String = conn
            .query_row(
                "SELECT created_at FROM tasks WHERE id = ?1",
                [task_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self::elapsed(
            Self::parse_timestamp(&created_at)?,
            Self::parse_timestamp(ended_at)?,
        )
        .num_seconds())
    }

    fn get_task(conn: &Connection, task_id: &str) -> Result<Option<TaskDto>, String> {
        match conn.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
            [task_id],
            row_to_task,
        ) {
            Ok(task) => Ok(Some(task)),
            Err(SqlError::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn task_with_status(
        conn: &Connection,
        status: &str,
        order_by: &str,
    ) -> Result<Option<TaskDto>, String> {
        let sql = format!("SELECT id FROM tasks WHERE status = ?1 ORDER BY {order_by} LIMIT 1");
        let id: Option<String> = match conn.query_row(&sql, [status], |r| r.get(0)) {
            Ok(id) => Some(id),
            Err(SqlError::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.to_string()),
        };
        match id {
            Some(id) => Self::get_task(conn, &id),
            None => Ok(None),
        }
    }

    fn running_task(conn: &Connection) -> Result<Option<TaskDto>, String> {
        Self::task_with_status(conn, "running", "created_at DESC")
    }

    fn latest_paused(conn: &Connection) -> Result<Option<TaskDto>, String> {
        Self::task_with_status(conn, "paused", "created_at DESC")
    }

    fn active_task(conn: &Connection) -> Result<Option<TaskDto>, String> {
        match Self::running_task(conn)? {
            Some(task) => Ok(Some(task)),
            None => Self::latest_paused(conn),
        }
    }

    fn require_active(conn: &Connection) -> Result<TaskDto, String> {
        Self::active_task(conn)?.ok_or_else(|| "no active task".into())
    }

    fn append_closed_segment(
        conn: &Connection,
        task_id: &str,
        ended_at: &str,
    ) -> Result<(), String> {
        let started_at = Self::latest_start_at(conn, task_id)?
            .ok_or_else(|| "running task has no start event".to_string())?;
        // Keep a zero-length segment on rollback (or equal timestamps), never a negative one.
        // Compare parsed instants, not RFC3339 strings, which may use different offsets.
        let end = chrono::DateTime::parse_from_rfc3339(ended_at).map_err(|e| e.to_string())?;
        let start = chrono::DateTime::parse_from_rfc3339(&started_at).map_err(|e| e.to_string())?;
        let ended_at = if end <= start {
            started_at.as_str()
        } else {
            ended_at
        };
        conn.execute(
            "INSERT INTO segments (task_id, started_at, ended_at, kind)
             VALUES (?1, ?2, ?3, 'work')",
            rusqlite::params![task_id, started_at, ended_at],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    fn paused_since(conn: &Connection, task_id: &str) -> Result<Option<String>, String> {
        match conn.query_row(
            "SELECT at FROM events WHERE task_id=?1 AND kind='paused' ORDER BY id DESC LIMIT 1",
            [task_id],
            |r| r.get(0),
        ) {
            Ok(at) => Ok(Some(at)),
            Err(SqlError::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// 补齐 net_seconds 的实时计算，返回给前端
    fn with_live_net(conn: &Connection, mut task: TaskDto) -> Result<TaskDto, String> {
        task.net_seconds = Self::net_seconds(conn, &task)?;
        task.paused_since = if task.status == "paused" {
            Self::paused_since(conn, &task.id)?
        } else {
            None
        };
        Ok(task)
    }

    pub fn create_task(
        &self,
        name: &str,
        est_seconds: i64,
        tag: Option<&str>,
        parent_task_id: Option<&str>,
    ) -> Result<TaskDto, String> {
        if name.trim().is_empty() {
            return Err("task name is required".into());
        }
        if est_seconds <= 0 {
            return Err("est_seconds must be positive".into());
        }
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if Self::running_task(&tx)?.is_some() {
            return Err("a task is already running".into());
        }
        if let Some(parent_task_id) = parent_task_id {
            let valid_parent: bool = tx
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM tasks
                        WHERE id = ?1 AND status NOT IN ('done','abandoned')
                    )",
                    [parent_task_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !valid_parent {
                return Err("parent task not found or terminal".into());
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        tx.execute(
            "INSERT INTO tasks (id, name, est_seconds, status, created_at, tag, parent_task_id)
             VALUES (?1, ?2, ?3, 'running', ?4, ?5, ?6)",
            rusqlite::params![id, name.trim(), est_seconds, ts, tag, parent_task_id],
        )
        .map_err(|e| e.to_string())?;
        Self::add_event(&tx, &id, "created", None)?;
        Self::add_event(&tx, &id, "started", None)?;
        let task = Self::get_task(&tx, &id)?.ok_or_else(|| "created task not found".to_string())?;
        let task = Self::with_live_net(&tx, task)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(task)
    }

    pub fn pause(&self, reason: &str) -> Result<TaskDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::require_active(&tx)?;
        if !PAUSE_REASONS.contains(&reason) {
            return Err(format!("invalid pause reason: {reason}"));
        }
        if task.status != "running" {
            return Err(format!("cannot pause task in status {}", task.status));
        }
        let ended_at = now();
        Self::append_closed_segment(&tx, &task.id, &ended_at)?;
        let detail = serde_json::json!({ "reason": reason }).to_string();
        Self::add_event(&tx, &task.id, "paused", Some(&detail))?;
        tx.execute("UPDATE tasks SET status='paused' WHERE id=?1", [&task.id])
            .map_err(|e| e.to_string())?;
        let task =
            Self::get_task(&tx, &task.id)?.ok_or_else(|| "paused task not found".to_string())?;
        let task = Self::with_live_net(&tx, task)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(task)
    }

    pub fn resume(&self, expected_task_id: &str) -> Result<TaskDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::require_active(&tx)?;
        if task.id != expected_task_id {
            return Err("task changed, please refresh".into());
        }
        if task.status != "paused" {
            return Err(format!("cannot resume task in status {}", task.status));
        }
        Self::add_event(&tx, &task.id, "resumed", None)?;
        tx.execute("UPDATE tasks SET status='running' WHERE id=?1", [&task.id])
            .map_err(|e| e.to_string())?;
        let task =
            Self::get_task(&tx, &task.id)?.ok_or_else(|| "resumed task not found".to_string())?;
        let task = Self::with_live_net(&tx, task)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(task)
    }

    // A single supplied instant makes finalization consistent and deterministic in tests.
    fn terminate(
        &self,
        expected_task_id: &str,
        status: &str,
        kind: &str,
        finished_at: &str,
    ) -> Result<TaskDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::require_active(&tx)?;
        if task.id != expected_task_id {
            // Dropping this uncommitted transaction rolls it back without any writes.
            return Err("task changed, please refresh".into());
        }
        if task.status == "running" {
            Self::append_closed_segment(&tx, &task.id, finished_at)?;
        }
        let net = Self::closed_net_seconds(&tx, &task.id)?;
        let gross = Self::gross_seconds(&tx, &task.id, finished_at)?;
        tx.execute(
            "UPDATE tasks SET status=?2, finished_at=?3, net_seconds=?4, gross_seconds=?5 WHERE id=?1",
            rusqlite::params![task.id, status, finished_at, net, gross],
        )
        .map_err(|e| e.to_string())?;
        Self::add_event(&tx, &task.id, kind, None)?;
        let task =
            Self::get_task(&tx, &task.id)?.ok_or_else(|| "finished task not found".to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(task)
    }

    pub fn finish(&self, expected_task_id: &str) -> Result<TaskDto, String> {
        self.terminate(expected_task_id, "done", "finished", &now())
    }

    pub fn abandon(&self, expected_task_id: &str) -> Result<TaskDto, String> {
        self.terminate(expected_task_id, "abandoned", "abandoned", &now())
    }

    pub fn current(&self) -> Result<Option<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        match Self::active_task(&conn)? {
            Some(task) => Ok(Some(Self::with_live_net(&conn, task)?)),
            None => Ok(None),
        }
    }

    pub fn list_paused(&self) -> Result<Vec<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE status='paused' ORDER BY created_at DESC"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_task).map_err(|e| e.to_string())?;
        rows.map(|row| {
            let task = row.map_err(|e| e.to_string())?;
            Self::with_live_net(&conn, task)
        })
        .collect()
    }

    fn get_setting_from_conn(conn: &Connection, key: &str) -> Result<Option<String>, String> {
        match conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        }) {
            Ok(value) => Ok(Some(value)),
            Err(SqlError::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub(crate) fn snapshot_overdue_upgrade(
        conn: &Connection,
        backfill_v5: bool,
    ) -> Result<(), String> {
        let at = Self::get_setting_from_conn(conn, OVERDUE_UPGRADE_CUTOFF_KEY)?
            .ok_or("missing overdue upgrade cutoff")?;
        let at = Self::parse_timestamp(&at)?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TASK_COLUMNS}, overdue_schedule_started_at FROM tasks
             WHERE status IN ('running', 'paused') AND overdue_schedule_started_at IS NOT NULL"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row_to_task(row)?,
                    row.get::<_, String>("overdue_schedule_started_at")?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let tasks = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (task, v5_at) in tasks {
            let cutoff_at = at.max(Self::parse_timestamp(&v5_at)?);
            let last_legacy_fire = if backfill_v5 {
                Self::last_legacy_overdue_fire(conn, &task.id)?
            } else {
                None
            };
            let mut closed = TimeDelta::zero();
            let mut legacy_peak = TimeDelta::zero();
            Self::visit_closed_work(conn, &task.id, |start, end| {
                // v5 may already have closed a formerly live segment using a rolled-back clock.
                // Its legacy fire had the preceding closed work plus the live time at firing.
                // We don't know which segment was live: take the greatest possible total.
                if let Some(fired_at) = last_legacy_fire {
                    legacy_peak = legacy_peak.max(closed + Self::elapsed(start, fired_at));
                }
                closed += Self::elapsed(start, end);
            })?;
            let cutoff = if task.status == "running" {
                let start = Self::running_start(conn, &task)?;
                closed
                    + Self::elapsed(
                        start,
                        last_legacy_fire.map_or(cutoff_at, |fired_at| cutoff_at.max(fired_at)),
                    )
            } else {
                closed
            };
            conn.execute(
                "UPDATE tasks SET overdue_upgrade_cutoff_net=?2 WHERE id=?1",
                rusqlite::params![task.id, cutoff.max(legacy_peak).num_seconds()],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn last_legacy_overdue_fire(
        conn: &Connection,
        task_id: &str,
    ) -> Result<Option<DateTime<FixedOffset>>, String> {
        let mut stmt = conn.prepare(
            "SELECT fired_at FROM reminder_fired WHERE task_id=?1 AND node GLOB 'overdue:[0-9]*'"
        ).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([task_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut latest = None;
        for at in rows {
            latest = latest.max(Some(Self::parse_timestamp(
                &at.map_err(|e| e.to_string())?,
            )?));
        }
        Ok(latest)
    }

    fn reminder_config_from_conn(conn: &Connection) -> Result<ReminderConfig, String> {
        ReminderConfig::from_settings(|key| Self::get_setting_from_conn(conn, key))
    }

    fn reminder_candidates(
        conn: &Connection,
        task: &TaskDto,
        net_seconds: i64,
        config: ReminderConfig,
    ) -> Result<Vec<DueReminder>, String> {
        let (interval_changed_at_net, upgrade_cutoff_net): (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT overdue_interval_changed_at_net, overdue_upgrade_cutoff_net FROM tasks WHERE id=?1",
                [&task.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        let overdue_from_net = interval_changed_at_net
            .into_iter()
            .chain(upgrade_cutoff_net.map(|net| net + 1))
            .max();
        let task_config = config.for_task(task.est_seconds, overdue_from_net);
        Ok(due_reminders(net_seconds, &task_config))
    }

    pub fn check_reminders(&self, expected_task_id: &str) -> Result<ReminderCheckDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let config = Self::reminder_config_from_conn(&tx)?;
        let config_dto = ReminderConfigDto::from(&config);
        let task = Self::require_active(&tx)?;
        if task.id != expected_task_id {
            return Err("task changed, please refresh".into());
        }
        let net_seconds = Self::net_seconds(&tx, &task)?;
        if task.status != "running" {
            return Ok(ReminderCheckDto {
                net_seconds,
                config: config_dto,
                reminders: Vec::new(),
            });
        }

        let candidates = Self::reminder_candidates(&tx, &task, net_seconds, config)?;
        let fired_at = now();
        let mut reminders = Vec::new();
        for mut candidate in candidates {
            let inserted = tx
                .execute(
                    "INSERT INTO reminder_fired (task_id, node, fired_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(task_id, node) DO NOTHING",
                    rusqlite::params![task.id, candidate.node, fired_at],
                )
                .map_err(|e| e.to_string())?;
            if inserted == 0 {
                continue;
            }
            let detail = serde_json::json!({ "node": candidate.node, "level": candidate.level });
            Self::add_event(&tx, &task.id, "reminder_fired", Some(&detail.to_string()))?;
            candidate.notify = candidate.notification_allowed(task.reminder_muted);
            reminders.push(candidate);
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(ReminderCheckDto {
            net_seconds,
            config: config_dto,
            reminders,
        })
    }

    pub fn with_reminder_notification(
        &self,
        task_id: &str,
        node: &str,
        level: ReminderLevel,
        send: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        match level {
            ReminderLevel::Due if node == "due" => (),
            ReminderLevel::Overdue if node.starts_with("overdue:at:") => (),
            _ => return Err("invalid notification reminder level or node".into()),
        }
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let task = Self::require_active(&conn)?;
        if task.id != task_id || task.status != "running" {
            return Err("task changed, please refresh".into());
        }
        let fired: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM reminder_fired WHERE task_id=?1 AND node=?2)",
                [task_id, node],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !fired {
            return Err("reminder has not fired for this task".into());
        }
        let config = Self::reminder_config_from_conn(&conn)?;
        let net_seconds = Self::net_seconds(&conn, &task)?;
        let candidates = Self::reminder_candidates(&conn, &task, net_seconds, config)?;
        if candidates.iter().any(|r| {
            r.node == node && r.level == level && r.notification_allowed(task.reminder_muted)
        }) {
            // Serialize the final decision and dispatch with task, settings and mute writes.
            send()?;
        }
        Ok(())
    }

    pub fn set_reminder_muted(
        &self,
        expected_task_id: &str,
        muted: bool,
    ) -> Result<TaskDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::get_task(&tx, expected_task_id)?.ok_or("task not found")?;
        if task.status == "done" || task.status == "abandoned" {
            return Err("cannot mute reminders for terminal task".into());
        }
        tx.execute(
            "UPDATE tasks SET reminder_muted=?2 WHERE id=?1",
            rusqlite::params![expected_task_id, i64::from(muted)],
        )
        .map_err(|e| e.to_string())?;
        let task = Self::get_task(&tx, expected_task_id)?
            .ok_or_else(|| "updated task not found".to_string())?;
        let task = Self::with_live_net(&tx, task)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(task)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        Self::get_setting_from_conn(&conn, key)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.set_settings(&[(key.to_owned(), value.to_owned())].into())
    }

    pub fn set_settings(
        &self,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        if values.contains_key(OVERDUE_UPGRADE_CUTOFF_KEY) {
            return Err("overdue upgrade cutoff is migration-owned".into());
        }
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let interval_changed = if let Some(value) = values.get(OVERDUE_INTERVAL_KEY) {
            let previous = Self::get_setting_from_conn(&tx, OVERDUE_INTERVAL_KEY)?;
            parse_overdue_interval(previous.as_deref()).ok()
                != Some(parse_overdue_interval(Some(value))?)
        } else {
            false
        };
        for (key, value) in values {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [key, value],
            )
            .map_err(|e| e.to_string())?;
        }
        if interval_changed {
            Self::snapshot_overdue_interval_change(&tx)?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    fn snapshot_overdue_interval_change(conn: &Connection) -> Result<(), String> {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TASK_COLUMNS} FROM tasks WHERE status IN ('running', 'paused')"
            ))
            .map_err(|e| e.to_string())?;
        let tasks = stmt
            .query_map([], row_to_task)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for task in tasks {
            let net = Self::net_seconds(conn, &task)?;
            conn.execute(
                "UPDATE tasks SET overdue_interval_changed_at_net=?2 WHERE id=?1",
                rusqlite::params![task.id, net],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn task_detail(&self, task_id: &str) -> Result<TaskDetailDto, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let task = match Self::get_task(&conn, task_id)? {
            Some(t) => Self::with_live_net(&conn, t)?,
            None => return Err("task not found".into()),
        };
        let mut stmt = conn
            .prepare(
                "SELECT started_at, ended_at, kind FROM segments WHERE task_id=?1 ORDER BY started_at, id",
            )
            .map_err(|e| e.to_string())?;
        let segments = stmt
            .query_map([task_id], |r| {
                Ok(SegmentDto {
                    started_at: r.get(0)?,
                    ended_at: r.get(1)?,
                    kind: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, at, kind, detail FROM events WHERE task_id=?1 ORDER BY id")
            .map_err(|e| e.to_string())?;
        let events = stmt
            .query_map([task_id], |r| {
                Ok(EventDto {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    kind: r.get(2)?,
                    detail: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(TaskDetailDto {
            task,
            segments,
            events,
        })
    }

    /// Soft deletion changes visibility only. No trajectory rows or events are changed.
    pub fn delete_task(&self, task_id: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::get_task(&tx, task_id)?.ok_or("task not found")?;
        if task.status != "done" && task.status != "abandoned" {
            return Err("only finished tasks can be deleted".into());
        }
        // Repeating a deletion is intentionally idempotent.
        tx.execute("UPDATE tasks SET hidden=1 WHERE id=?1", [task_id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    /// Transactional, all-or-nothing deletion. An empty list is a deliberate no-op.
    pub fn delete_tasks(&self, ids: &[String]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for task_id in ids {
            let task = Self::get_task(&tx, task_id)?.ok_or("task not found")?;
            if task.status != "done" && task.status != "abandoned" {
                // Dropping tx without commit rolls back any ids already updated.
                return Err("only finished tasks can be deleted".into());
            }
            tx.execute("UPDATE tasks SET hidden=1 WHERE id=?1", [task_id])
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn list_finished(&self) -> Result<Vec<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE hidden=0 AND status IN ('done','abandoned') ORDER BY finished_at DESC"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_task).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    fn t() -> Tracker {
        Tracker::new(store::open_in_memory().unwrap())
    }

    fn advance_running_task(tr: &Tracker, task_id: &str, elapsed: i64) {
        let conn = tr.conn.lock().unwrap();
        let past = (chrono::Utc::now() - chrono::Duration::seconds(elapsed)).to_rfc3339();
        conn.execute(
            "UPDATE events SET at=?1 WHERE task_id=?2 AND kind='started'",
            [&past, task_id],
        )
        .unwrap();
    }

    fn fired_nodes(tr: &Tracker, task_id: &str) -> Vec<String> {
        tr.check_reminders(task_id)
            .unwrap()
            .reminders
            .into_iter()
            .map(|r| r.node)
            .collect()
    }

    #[test]
    fn check_reminders_uses_absolute_milestones_and_does_not_backfill_on_interval_edits() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        advance_running_task(&tr, &task.id, 662);
        assert_eq!(
            fired_nodes(&tr, &task.id),
            ["warn", "near", "due", "overdue:at:600"]
        );
        assert!(fired_nodes(&tr, &task.id).is_empty());

        // Switch from 10m to 12m after the 600s milestone. The new first milestone
        // has the same ordinal but a different absolute time; the poll may run late.
        tr.set_settings(&[("reminder_overdue_interval_minutes".into(), "12".into())].into())
            .unwrap();
        assert!(fired_nodes(&tr, &task.id).is_empty());
        advance_running_task(&tr, &task.id, 785);
        assert_eq!(fired_nodes(&tr, &task.id), ["overdue:at:720"]);

        // 660s is already past when switching to 11m, so do not fire it.
        tr.set_setting("reminder_overdue_interval_minutes", "11")
            .unwrap();
        assert!(fired_nodes(&tr, &task.id).is_empty());
        advance_running_task(&tr, &task.id, 1382);
        assert_eq!(fired_nodes(&tr, &task.id), ["overdue:at:1320"]);
        assert!(fired_nodes(&tr, &task.id).is_empty());
    }

    #[test]
    fn task_mute_suppresses_overdue_but_not_due_notification() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        tr.set_reminder_muted(&task.id, true).unwrap();
        advance_running_task(&tr, &task.id, 662);
        let reminders = tr.check_reminders(&task.id).unwrap().reminders;
        assert!(
            reminders
                .iter()
                .find(|r| r.level == ReminderLevel::Due)
                .unwrap()
                .notify
        );
        assert!(
            !reminders
                .iter()
                .find(|r| r.level == ReminderLevel::Overdue)
                .unwrap()
                .notify
        );
    }

    #[test]
    fn new_reminders_append_history_once_in_firing_order() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        advance_running_task(&tr, &task.id, 662);
        for _ in 0..2 {
            tr.check_reminders(&task.id).unwrap();
        }
        let detail = tr.task_detail(&task.id).unwrap();
        let reminders: Vec<_> = detail
            .events
            .iter()
            .filter(|event| event.kind == "reminder_fired")
            .map(|event| {
                serde_json::from_str::<serde_json::Value>(event.detail.as_deref().unwrap()).unwrap()
            })
            .collect();
        assert_eq!(
            reminders,
            [
                serde_json::json!({"node": "warn", "level": "warn"}),
                serde_json::json!({"node": "near", "level": "near"}),
                serde_json::json!({"node": "due", "level": "due"}),
                serde_json::json!({"node": "overdue:at:600", "level": "overdue"}),
            ]
        );
    }

    #[test]
    fn failed_history_append_rolls_back_reminders_so_they_can_retry() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        advance_running_task(&tr, &task.id, 80);
        tr.conn
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_due BEFORE INSERT ON events
             WHEN NEW.kind = 'reminder_fired' AND json_extract(NEW.detail, '$.node') = 'due'
             BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;",
            )
            .unwrap();
        assert!(tr.check_reminders(&task.id).is_err());
        assert!(
            tr.task_detail(&task.id)
                .unwrap()
                .events
                .iter()
                .all(|e| e.kind != "reminder_fired")
        );
        tr.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_due;")
            .unwrap();
        assert_eq!(fired_nodes(&tr, &task.id), ["warn", "near", "due"]);
    }

    #[test]
    fn paused_checks_return_current_config_without_firing() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        advance_running_task(&tr, &task.id, 662);
        let paused = tr.pause("rest").unwrap();
        tr.set_setting("reminder_due_percent", "150").unwrap();
        let checked = tr.check_reminders(&task.id).unwrap();
        assert_eq!(checked.config.due_percent, 150);
        assert_eq!(checked.net_seconds, paused.net_seconds);
        assert!(checked.reminders.is_empty());
        assert!(
            tr.task_detail(&task.id)
                .unwrap()
                .events
                .iter()
                .all(|e| e.kind != "reminder_fired")
        );
    }

    #[test]
    fn notification_gate_rejects_non_notification_levels_and_wrong_context() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        advance_running_task(&tr, &task.id, 662);
        let never_send = || panic!("invalid reminder must not reach notification delivery");
        assert!(
            tr.with_reminder_notification(&task.id, "due", ReminderLevel::Due, never_send)
                .is_err()
        );
        tr.check_reminders(&task.id).unwrap();
        for (id, node, level) in [
            (task.id.as_str(), "warn", ReminderLevel::Warn),
            (task.id.as_str(), "near", ReminderLevel::Near),
            (task.id.as_str(), "near", ReminderLevel::Due),
            (task.id.as_str(), "overdue:at:600", ReminderLevel::Due),
            (task.id.as_str(), "overdue:at:1200", ReminderLevel::Overdue),
            ("another-task", "due", ReminderLevel::Due),
        ] {
            assert!(
                tr.with_reminder_notification(id, node, level, never_send)
                    .is_err()
            );
        }
        tr.pause("rest").unwrap();
        assert!(
            tr.with_reminder_notification(&task.id, "due", ReminderLevel::Due, never_send)
                .is_err()
        );
        tr.finish(&task.id).unwrap();
        assert!(
            tr.with_reminder_notification(&task.id, "due", ReminderLevel::Due, never_send)
                .is_err()
        );
    }

    #[test]
    fn notification_gate_rechecks_current_settings_and_task_mute_before_delivering() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        advance_running_task(&tr, &task.id, 662);
        tr.check_reminders(&task.id).unwrap();
        let sent = std::cell::Cell::new(0);
        let send = || {
            sent.set(sent.get() + 1);
            Ok(())
        };

        tr.with_reminder_notification(&task.id, "overdue:at:600", ReminderLevel::Overdue, send)
            .unwrap();
        assert_eq!(sent.get(), 1);
        tr.set_reminder_muted(&task.id, true).unwrap();
        tr.with_reminder_notification(&task.id, "overdue:at:600", ReminderLevel::Overdue, send)
            .unwrap();
        assert_eq!(sent.get(), 1);
        tr.with_reminder_notification(&task.id, "due", ReminderLevel::Due, send)
            .unwrap();
        assert_eq!(sent.get(), 2);

        tr.set_setting("reminder_due_notify", "false").unwrap();
        tr.set_setting("reminder_overdue_notify", "false").unwrap();
        tr.set_reminder_muted(&task.id, false).unwrap();
        tr.with_reminder_notification(&task.id, "due", ReminderLevel::Due, send)
            .unwrap();
        tr.with_reminder_notification(&task.id, "overdue:at:600", ReminderLevel::Overdue, send)
            .unwrap();
        assert_eq!(sent.get(), 2);
    }

    #[test]
    fn bulk_soft_delete_applies_every_finished_id_and_rejects_as_a_whole() {
        let tr = t();
        let a = tr.create_task("bulk-a", 60, None, None).unwrap();
        tr.finish(&a.id).unwrap();
        let b = tr.create_task("bulk-b", 60, None, None).unwrap();
        tr.abandon(&b.id).unwrap();
        tr.delete_tasks(&[a.id.clone(), b.id.clone()]).unwrap();
        let finished = tr.list_finished().unwrap();
        assert!(
            !finished
                .iter()
                .any(|task| [a.id.clone(), b.id.clone()].contains(&task.id))
        );
        let conn = tr.conn.lock().unwrap();
        for id in [a.id, b.id] {
            assert_eq!(
                conn.query_row("SELECT hidden FROM tasks WHERE id=?1", [&id], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }

    #[test]
    fn bulk_soft_delete_with_nonterminal_id_rejects_and_hides_nothing() {
        let tr = t();
        let done = tr.create_task("done", 60, None, None).unwrap();
        tr.finish(&done.id).unwrap();
        let running = tr.create_task("running", 60, None, None).unwrap();
        assert_eq!(
            tr.delete_tasks(&[done.id.clone(), running.id.clone()])
                .unwrap_err(),
            "only finished tasks can be deleted"
        );
        assert!(
            tr.list_finished()
                .unwrap()
                .iter()
                .any(|task| task.id == done.id)
        );
        let conn = tr.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT hidden FROM tasks WHERE id=?1", [&done.id], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn empty_bulk_soft_delete_is_a_no_op() {
        let tr = t();
        let task = tr.create_task("keep", 60, None, None).unwrap();
        tr.finish(&task.id).unwrap();
        tr.delete_tasks(&[]).unwrap();
        assert!(
            tr.list_finished()
                .unwrap()
                .iter()
                .any(|item| item.id == task.id)
        );
    }

    #[test]
    fn soft_delete_preserves_trajectory_is_idempotent_and_filters_history() {
        let tr = t();
        for abandoned in [false, true] {
            let task = tr.create_task("delete", 60, None, None).unwrap();
            if abandoned {
                tr.abandon(&task.id).unwrap();
            } else {
                tr.finish(&task.id).unwrap();
            }
            let before = serde_json::to_value(tr.task_detail(&task.id).unwrap()).unwrap();
            assert!(tr.list_finished().unwrap().iter().any(|t| t.id == task.id));
            tr.delete_task(&task.id).unwrap();
            tr.delete_task(&task.id).unwrap();
            assert!(!tr.list_finished().unwrap().iter().any(|t| t.id == task.id));
            assert_eq!(
                serde_json::to_value(tr.task_detail(&task.id).unwrap()).unwrap(),
                before
            );
            let conn = tr.conn.lock().unwrap();
            assert_eq!(
                conn.query_row("SELECT hidden FROM tasks WHERE id=?1", [&task.id], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }

    #[test]
    fn soft_delete_rejects_running_paused_and_missing_tasks() {
        let tr = t();
        let task = tr.create_task("keep", 60, None, None).unwrap();
        assert_eq!(
            tr.delete_task(&task.id).unwrap_err(),
            "only finished tasks can be deleted"
        );
        tr.pause("rest").unwrap();
        assert_eq!(
            tr.delete_task(&task.id).unwrap_err(),
            "only finished tasks can be deleted"
        );
        assert_eq!(tr.task_detail(&task.id).unwrap().task.status, "paused");
        assert_eq!(tr.delete_task("missing").unwrap_err(), "task not found");
    }

    #[test]
    fn stale_resume_does_not_resume_a_different_paused_task() {
        let tr = t();
        let a = tr.create_task("A", 60, None, None).unwrap();
        tr.pause("switch").unwrap();
        let b = tr.create_task("B", 60, None, Some(&a.id)).unwrap();
        tr.pause("rest").unwrap();
        assert_eq!(
            tr.resume(&a.id).unwrap_err(),
            "task changed, please refresh"
        );
        assert_eq!(tr.task_detail(&a.id).unwrap().task.status, "paused");
        assert_eq!(tr.task_detail(&b.id).unwrap().task.status, "paused");
    }

    #[test]
    fn task_detail_preserves_event_append_order_when_clock_rolls_back() {
        let tr = t();
        let task = tr
            .create_task("clock rollback history", 60, None, None)
            .unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute("DELETE FROM events WHERE task_id=?1", [&task.id])
                .unwrap();
            for (at, kind) in [
                ("2026-11-26T00:00:20Z", "paused"),
                ("2026-11-26T00:00:10Z", "resumed"),
                ("2026-11-26T00:00:40Z", "finished"),
            ] {
                conn.execute(
                    "INSERT INTO events (task_id, at, kind) VALUES (?1, ?2, ?3)",
                    rusqlite::params![task.id, at, kind],
                )
                .unwrap();
            }
        }
        let detail = tr.task_detail(&task.id).unwrap();
        assert_eq!(
            detail
                .events
                .iter()
                .map(|e| e.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["paused", "resumed", "finished"]
        );
        assert!(detail.events.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(detail.events[0].id > 0);
    }

    #[test]
    fn repeated_finish_of_child_does_not_finish_paused_parent() {
        let tr = t();
        let parent = tr.create_task("A", 60, None, None).unwrap();
        tr.pause("switch").unwrap();
        let child = tr.create_task("B", 60, None, Some(&parent.id)).unwrap();
        assert_eq!(tr.finish(&child.id).unwrap().id, child.id);
        assert_eq!(
            tr.finish(&child.id).unwrap_err(),
            "task changed, please refresh"
        );
        let detail = tr.task_detail(&parent.id).unwrap();
        assert_eq!(detail.task.status, "paused");
        assert!(detail.task.finished_at.is_none());
        assert!(
            !detail
                .events
                .iter()
                .any(|e| e.kind == "finished" || e.kind == "abandoned")
        );
        assert_eq!(tr.list_finished().unwrap().len(), 1);
    }

    #[test]
    fn repeated_abandon_of_child_does_not_abandon_paused_parent() {
        let tr = t();
        let parent = tr.create_task("A", 60, None, None).unwrap();
        tr.pause("switch").unwrap();
        let child = tr.create_task("B", 60, None, Some(&parent.id)).unwrap();
        assert_eq!(tr.abandon(&child.id).unwrap().id, child.id);
        assert_eq!(
            tr.abandon(&child.id).unwrap_err(),
            "task changed, please refresh"
        );
        let detail = tr.task_detail(&parent.id).unwrap();
        assert_eq!(detail.task.status, "paused");
        assert!(detail.task.finished_at.is_none());
        assert!(
            !detail
                .events
                .iter()
                .any(|e| e.kind == "finished" || e.kind == "abandoned")
        );
        assert_eq!(tr.list_finished().unwrap().len(), 1);
    }

    #[test]
    fn subsecond_segments_are_summed_before_rounding_and_persisted() {
        let tr = t();
        let task = tr.create_task("fractional", 60, None, None).unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute(
                "UPDATE tasks SET status='paused', created_at='2026-11-26T00:00:00.900Z' WHERE id=?1",
                [&task.id],
            )
            .unwrap();
            // Each 0.2 s interval crosses a second boundary: endpoint truncation yields 3 s.
            for (start, end) in [
                ("2026-11-26T00:00:00.900Z", "2026-11-26T00:00:01.100Z"),
                ("2026-11-26T00:00:02.900Z", "2026-11-26T00:00:03.100Z"),
                ("2026-11-26T00:00:04.900Z", "2026-11-26T00:00:05.100Z"),
            ] {
                conn.execute(
                    "INSERT INTO segments (task_id, started_at, ended_at, kind) VALUES (?1, ?2, ?3, 'work')",
                    rusqlite::params![task.id, start, end],
                ).unwrap();
            }
        }
        assert_eq!(tr.current().unwrap().unwrap().net_seconds, 0);
        let done = tr
            .terminate(&task.id, "done", "finished", "2026-11-26T00:00:05.100Z")
            .unwrap();
        assert_eq!(done.net_seconds, 0);
        let conn = tr.conn.lock().unwrap();
        let totals: (i64, i64) = conn
            .query_row(
                "SELECT net_seconds, gross_seconds FROM tasks WHERE id=?1",
                [&task.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // Gross is exactly 4.2 s: truncate the duration, not the two endpoints (5 s).
        assert_eq!(totals, (0, 4));
    }

    #[test]
    fn subsecond_remainders_are_not_discarded_per_segment() {
        let tr = t();
        let task = tr.create_task("fractional sum", 60, None, None).unwrap();
        let conn = tr.conn.lock().unwrap();
        for _ in 0..3 {
            conn.execute(
                "INSERT INTO segments (task_id, started_at, ended_at, kind)
                 VALUES (?1, '2026-11-26T00:00:00.100Z', '2026-11-26T00:00:00.700Z', 'work')",
                [&task.id],
            )
            .unwrap();
        }
        assert_eq!(Tracker::closed_net_seconds(&conn, &task.id).unwrap(), 1);
    }

    #[test]
    fn gross_duration_rounds_elapsed_time_not_endpoints() {
        let tr = t();
        let task = tr.create_task("gross", 60, None, None).unwrap();
        let conn = tr.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET created_at='2026-11-26T00:00:00.900Z' WHERE id=?1",
            [&task.id],
        )
        .unwrap();
        for (end, expected) in [
            ("2026-11-26T00:00:01.100Z", 0),
            ("2026-11-26T00:00:02.100Z", 1),
            ("2026-11-26T00:00:00.100Z", 0),
        ] {
            assert_eq!(
                Tracker::gross_seconds(&conn, &task.id, end).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn rollback_uses_append_order_and_clamps_closed_segments() {
        let tr = t();
        let task = tr.create_task("rollback", 60, None, None).unwrap();
        let conn = tr.conn.lock().unwrap();
        conn.execute("DELETE FROM events WHERE task_id=?1", [&task.id])
            .unwrap();
        for (at, kind) in [
            ("2026-11-26T00:00:10Z", "started"),
            ("2026-11-26T00:00:11Z", "paused"),
            ("2026-11-26T00:00:05Z", "resumed"),
        ] {
            conn.execute(
                "INSERT INTO events (task_id, at, kind) VALUES (?1, ?2, ?3)",
                rusqlite::params![task.id, at, kind],
            )
            .unwrap();
        }
        Tracker::append_closed_segment(&conn, &task.id, "2026-11-26T00:00:06Z").unwrap();
        let first: (String, String) = conn
            .query_row(
                "SELECT started_at, ended_at FROM segments WHERE task_id=?1 ORDER BY id LIMIT 1",
                [&task.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            first,
            ("2026-11-26T00:00:05Z".into(), "2026-11-26T00:00:06Z".into())
        );
        for end in ["2026-11-26T00:00:04Z", "2026-11-26T00:00:05Z"] {
            Tracker::append_closed_segment(&conn, &task.id, end).unwrap();
            let closed: (String, String) = conn.query_row(
                "SELECT started_at, ended_at FROM segments WHERE task_id=?1 ORDER BY id DESC LIMIT 1",
                [&task.id], |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap();
            assert_eq!(closed.0, closed.1);
        }
        assert_eq!(Tracker::closed_net_seconds(&conn, &task.id).unwrap(), 1);
    }

    #[test]
    fn finish_after_clock_rollback_persists_zero_totals() {
        let tr = t();
        let task = tr.create_task("rollback finish", 60, None, None).unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute(
                "UPDATE tasks SET created_at='2026-11-26T00:00:10Z' WHERE id=?1",
                [&task.id],
            )
            .unwrap();
            conn.execute("INSERT INTO events (task_id, at, kind) VALUES (?1, '2026-11-26T00:00:11Z', 'resumed')", [&task.id]).unwrap();
        }
        let done = tr
            .terminate(&task.id, "done", "finished", "2026-11-26T00:00:05Z")
            .unwrap();
        assert_eq!(done.net_seconds, 0);
        let conn = tr.conn.lock().unwrap();
        let totals: (i64, i64) = conn
            .query_row(
                "SELECT net_seconds, gross_seconds FROM tasks WHERE id=?1",
                [&task.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(totals, (0, 0));
    }

    #[test]
    fn create_starts_running_with_open_segment() {
        let tr = t();
        let task = tr.create_task("写周报", 3600, Some("科研"), None).unwrap();
        assert_eq!(task.status, "running");
        assert_eq!(task.name, "写周报");
        assert_eq!(task.tag.as_deref(), Some("科研"));
        // 段已开：current 可见，净耗时 >= 0
        let cur = tr.current().unwrap().unwrap();
        assert_eq!(cur.id, task.id);
        assert!(cur.net_seconds >= 0);
    }

    #[test]
    fn running_time_is_computed_from_latest_start_event() {
        let tr = t();
        tr.create_task("A", 60, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let running = tr.current().unwrap().unwrap();
        assert!(running.net_seconds >= 1, "net {}", running.net_seconds);
    }

    #[test]
    fn pause_appends_one_closed_segment() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM segments WHERE task_id=?1",
                    [&task.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "running segments must not be stored");
        }

        tr.pause("rest").unwrap();
        let conn = tr.conn.lock().unwrap();
        let (count, closed): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COUNT(ended_at) FROM segments WHERE task_id=?1",
                [&task.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(closed, 1);
    }

    #[test]
    fn single_task_model_rejects_second_task() {
        let tr = t();
        tr.create_task("A", 60, None, None).unwrap();
        let err = tr.create_task("B", 60, None, None).unwrap_err();
        assert_eq!(err, "a task is already running");
    }

    #[test]
    fn pause_resume_accumulates_only_work_segments() {
        let tr = t();
        let task = tr.create_task("A", 100, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        tr.pause("rest").unwrap();
        let paused = tr.current().unwrap().unwrap();
        assert_eq!(paused.status, "paused");
        assert!(paused.net_seconds >= 1, "net {}", paused.net_seconds);
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let still_paused = tr.current().unwrap().unwrap();
        assert_eq!(still_paused.net_seconds, paused.net_seconds);
        tr.resume(&task.id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let running = tr.current().unwrap().unwrap();
        // 第二段至少 1 秒；暂停的 1.2 秒不得计入
        assert!(
            running.net_seconds >= still_paused.net_seconds + 1,
            "net {}",
            running.net_seconds
        );
    }

    #[test]
    fn pause_without_active_task_precedes_reason_validation() {
        let tr = t();
        let err = tr.pause("bogus").unwrap_err();
        assert_eq!(err, "no active task");
    }

    #[test]
    fn pause_requires_reason_in_enum() {
        let tr = t();
        tr.create_task("A", 60, None, None).unwrap();
        let err = tr.pause("bogus").unwrap_err();
        assert!(err.contains("invalid pause reason"));
    }

    #[test]
    fn finish_fixes_totals_and_events() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let done = tr.finish(&task.id).unwrap();
        assert_eq!(done.status, "done");
        assert!(done.finished_at.is_some());
        assert!(done.net_seconds <= 1, "net {}", done.net_seconds);
        assert!(tr.current().unwrap().is_none()); // 无活动任务返回 None
        // 事件齐全
        let conn = tr.conn.lock().unwrap();
        let kinds: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT kind FROM events WHERE task_id=?1 ORDER BY at, id")
                .unwrap();
            let rows = stmt
                .query_map([&task.id], |r| r.get::<_, String>(0))
                .unwrap();
            rows.collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert_eq!(kinds, vec!["created", "started", "finished"]);
    }

    #[test]
    fn abandon_records_trajectory() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        let t2 = tr.abandon(&task.id).unwrap();
        assert_eq!(t2.status, "abandoned");
        assert!(t2.finished_at.is_some());
    }

    #[test]
    fn list_finished_orders_desc() {
        let tr = t();
        let a = tr.create_task("A", 60, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        tr.finish(&a.id).unwrap();
        let b = tr.create_task("B", 60, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        tr.finish(&b.id).unwrap();
        let list = tr.list_finished().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id);
        assert_eq!(list[1].id, a.id);
    }

    #[test]
    fn paused_task_does_not_block_new_task() {
        let tr = t();
        tr.create_task("A", 60, None, None).unwrap();
        tr.pause("rest").unwrap();
        // paused 不阻塞新建，且无父任务的新任务不带关联
        let b = tr.create_task("B", 60, None, None).unwrap();
        assert_eq!(b.status, "running");
        assert_eq!(tr.current().unwrap().unwrap().id, b.id); // running 优先
        let err = tr.create_task("C", 60, None, None).unwrap_err();
        assert_eq!(err, "a task is already running");
    }

    #[test]
    fn interruption_creates_linked_child_and_parent_stays_paused() {
        let tr = t();
        let parent = tr.create_task("A", 3600, None, None).unwrap();
        tr.pause("switch").unwrap();
        let child = tr
            .create_task("紧急修复", 900, Some("行政"), Some(&parent.id))
            .unwrap();
        assert_eq!(child.status, "running");
        let conn = tr.conn.lock().unwrap();
        let p: String = conn
            .query_row(
                "SELECT parent_task_id FROM tasks WHERE id=?1",
                [&child.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(p, parent.id);
        drop(conn);
        // current 显示子任务；子任务结束后回到父任务（paused，含 paused_since）
        assert_eq!(tr.current().unwrap().unwrap().id, child.id);
        tr.finish(&child.id).unwrap();
        let cur = tr.current().unwrap().unwrap();
        assert_eq!(cur.id, parent.id);
        assert_eq!(cur.status, "paused");
        assert!(cur.paused_since.is_some());
    }

    #[test]
    fn paused_since_exposed_on_current_paused_task() {
        let tr = t();
        tr.create_task("A", 60, None, None).unwrap();
        tr.pause("rest").unwrap();
        let cur = tr.current().unwrap().unwrap();
        assert!(cur.paused_since.is_some());
    }

    #[test]
    fn list_paused_orders_desc_and_running_task_finish_keeps_paused_sibling() {
        let tr = t();
        let a = tr.create_task("A", 60, None, None).unwrap();
        tr.pause("switch").unwrap();
        let b = tr.create_task("B", 60, None, Some(&a.id)).unwrap();
        tr.finish(&b.id).unwrap();
        let paused = tr.list_paused().unwrap();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0].id, a.id);
        // 再次打断验证排序：新 paused 任务排在前面
        let c = tr.create_task("C", 60, None, None).unwrap();
        tr.pause("rest").unwrap();
        let paused = tr.list_paused().unwrap();
        assert_eq!(paused.len(), 2);
        assert_eq!(paused[0].id, c.id);
    }

    #[test]
    fn settings_roundtrip() {
        let tr = t();
        assert_eq!(tr.get_setting("long_pause_hours").unwrap(), None);
        tr.set_setting("long_pause_hours", "1").unwrap();
        assert_eq!(
            tr.get_setting("long_pause_hours").unwrap().as_deref(),
            Some("1")
        );
    }

    #[test]
    fn settings_batch_rolls_back_when_later_write_fails() {
        let tr = t();
        tr.set_setting("a_first", "original").unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute_batch(
                "CREATE TEMP TRIGGER reject_late_setting BEFORE INSERT ON settings
                 WHEN NEW.key = 'z_fail'
                 BEGIN SELECT RAISE(ABORT, 'injected settings failure'); END;",
            )
            .unwrap();
        }
        let values = std::collections::BTreeMap::from([
            ("a_first".to_string(), "written".to_string()),
            ("z_fail".to_string(), "rejected".to_string()),
        ]);

        assert!(tr.set_settings(&values).is_err());
        assert_eq!(
            tr.get_setting("a_first").unwrap().as_deref(),
            Some("original")
        );
        assert_eq!(tr.get_setting("z_fail").unwrap(), None);
    }

    #[test]
    fn task_detail_returns_segments_and_events_in_order() {
        let tr = t();
        let task = tr.create_task("A", 60, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        tr.pause("rest").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        tr.resume(&task.id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        tr.finish(&task.id).unwrap();
        let d = tr.task_detail(&task.id).unwrap();
        assert_eq!(d.task.id, task.id);
        assert_eq!(d.segments.len(), 2);
        assert!(d.segments.iter().all(|s| s.kind == "work"));
        let kinds: Vec<&str> = d.events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["created", "started", "paused", "resumed", "finished"]
        );
        let err = tr.task_detail("nope").unwrap_err();
        assert_eq!(err, "task not found");
    }

    #[test]
    fn task_detail_propagates_segment_and_event_row_errors() {
        let tr = t();
        let task = tr.create_task("bad segment", 60, None, None).unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO segments (task_id, started_at, ended_at, kind) VALUES (?1, ?2, NULL, 'work')",
                rusqlite::params![task.id, "2026-11-26T00:00:00Z"],
            )
            .unwrap();
        }
        assert!(
            tr.task_detail(&task.id)
                .unwrap_err()
                .contains("Invalid column type")
        );

        let tr = t();
        let task = tr.create_task("bad event", 60, None, None).unwrap();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO events (task_id, at, kind, detail) VALUES (?1, ?2, 'bad', ?3)",
                rusqlite::params![task.id, "2026-11-26T00:00:00Z", vec![0xff_u8]],
            )
            .unwrap();
        }
        assert!(
            tr.task_detail(&task.id)
                .unwrap_err()
                .contains("Invalid column type")
        );
    }

    #[test]
    fn parent_validation_excludes_only_terminal_statuses() {
        let tr = t();

        let err = tr
            .create_task("missing child", 60, None, Some("missing"))
            .unwrap_err();
        assert_eq!(err, "parent task not found or terminal");

        let done_parent = tr.create_task("done parent", 60, None, None).unwrap();
        tr.finish(&done_parent.id).unwrap();
        let err = tr
            .create_task("done child", 60, None, Some(&done_parent.id))
            .unwrap_err();
        assert_eq!(err, "parent task not found or terminal");

        let abandoned_parent = tr.create_task("abandoned parent", 60, None, None).unwrap();
        tr.abandon(&abandoned_parent.id).unwrap();
        let err = tr
            .create_task("abandoned child", 60, None, Some(&abandoned_parent.id))
            .unwrap_err();
        assert_eq!(err, "parent task not found or terminal");

        let pending_parent_id = uuid::Uuid::new_v4().to_string();
        {
            let conn = tr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO tasks (id, name, est_seconds, status, created_at)\n                 VALUES (?1, 'pending parent', 60, 'pending', '2026-11-26T00:00:00Z')",
                [&pending_parent_id],
            )
            .unwrap();
        }
        let child = tr
            .create_task("pending child", 60, None, Some(&pending_parent_id))
            .unwrap();
        assert_eq!(child.status, "running");
        let conn = tr.conn.lock().unwrap();
        let linked_parent: String = conn
            .query_row(
                "SELECT parent_task_id FROM tasks WHERE id=?1",
                [&child.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked_parent, pending_parent_id);
    }
}
