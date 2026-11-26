use rusqlite::{Connection, Error as SqlError};
use serde::Serialize;
use std::sync::Mutex;

const PAUSE_REASONS: [&str; 4] = ["rest", "disturbance", "switch", "other"];
const TASK_COLUMNS: &str = "id, name, est_seconds, status, tag, COALESCE(net_seconds, 0) AS net_seconds, created_at, finished_at";

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

    fn elapsed(started_at: &str, ended_at: &str) -> Result<chrono::TimeDelta, String> {
        let start = chrono::DateTime::parse_from_rfc3339(started_at).map_err(|e| e.to_string())?;
        let end = chrono::DateTime::parse_from_rfc3339(ended_at).map_err(|e| e.to_string())?;
        Ok((end - start).max(chrono::TimeDelta::zero()))
    }

    fn closed_work_duration(conn: &Connection, task_id: &str) -> Result<chrono::TimeDelta, String> {
        let mut stmt = conn
            .prepare(
                "SELECT started_at, ended_at FROM segments
             WHERE task_id = ?1 AND kind = 'work' AND ended_at IS NOT NULL",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([task_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut total = chrono::TimeDelta::zero();
        for row in rows {
            let (start, end) = row.map_err(|e| e.to_string())?;
            total += Self::elapsed(&start, &end)?;
        }
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

    fn net_seconds(conn: &Connection, task: &TaskDto) -> Result<i64, String> {
        let closed = Self::closed_work_duration(conn, &task.id)?;
        if task.status != "running" {
            return Ok(closed.num_seconds());
        }
        let started_at = Self::latest_start_at(conn, &task.id)?
            .ok_or_else(|| "running task has no start event".to_string())?;
        let live = Self::elapsed(&started_at, &now())?;
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
        Ok(Self::elapsed(&created_at, ended_at)?.num_seconds())
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

    fn active_task(conn: &Connection) -> Result<Option<TaskDto>, String> {
        let id: Option<String> = match conn.query_row(
            "SELECT id FROM tasks WHERE status IN ('pending','running','paused') LIMIT 1",
            [],
            |r| r.get(0),
        ) {
            Ok(id) => Some(id),
            Err(SqlError::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.to_string()),
        };
        match id {
            Some(id) => Self::get_task(conn, &id),
            None => Ok(None),
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

    /// 补齐 net_seconds 的实时计算，返回给前端
    fn with_live_net(conn: &Connection, mut task: TaskDto) -> Result<TaskDto, String> {
        task.net_seconds = Self::net_seconds(conn, &task)?;
        Ok(task)
    }

    pub fn create_task(
        &self,
        name: &str,
        est_seconds: i64,
        tag: Option<&str>,
    ) -> Result<TaskDto, String> {
        if name.trim().is_empty() {
            return Err("task name is required".into());
        }
        if est_seconds <= 0 {
            return Err("est_seconds must be positive".into());
        }
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if Self::active_task(&tx)?.is_some() {
            return Err("another task is active".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        tx.execute(
            "INSERT INTO tasks (id, name, est_seconds, status, created_at, tag)
             VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
            rusqlite::params![id, name.trim(), est_seconds, ts, tag],
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

    pub fn resume(&self) -> Result<TaskDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::require_active(&tx)?;
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
    fn terminate(&self, status: &str, kind: &str, finished_at: &str) -> Result<TaskDto, String> {
        let mut conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let task = Self::require_active(&tx)?;
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

    pub fn finish(&self) -> Result<TaskDto, String> {
        self.terminate("done", "finished", &now())
    }

    pub fn abandon(&self) -> Result<TaskDto, String> {
        self.terminate("abandoned", "abandoned", &now())
    }

    pub fn current(&self) -> Result<Option<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        match Self::active_task(&conn)? {
            Some(task) => Ok(Some(Self::with_live_net(&conn, task)?)),
            None => Ok(None),
        }
    }

    pub fn list_finished(&self) -> Result<Vec<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE status IN ('done','abandoned') ORDER BY finished_at DESC"
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

    #[test]
    fn subsecond_segments_are_summed_before_rounding_and_persisted() {
        let tr = t();
        let task = tr.create_task("fractional", 60, None).unwrap();
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
            .terminate("done", "finished", "2026-11-26T00:00:05.100Z")
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
        let task = tr.create_task("fractional sum", 60, None).unwrap();
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
        let task = tr.create_task("gross", 60, None).unwrap();
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
        let task = tr.create_task("rollback", 60, None).unwrap();
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
        let task = tr.create_task("rollback finish", 60, None).unwrap();
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
            .terminate("done", "finished", "2026-11-26T00:00:05Z")
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
        let task = tr.create_task("写周报", 3600, Some("科研")).unwrap();
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
        tr.create_task("A", 60, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let running = tr.current().unwrap().unwrap();
        assert!(running.net_seconds >= 1, "net {}", running.net_seconds);
    }

    #[test]
    fn pause_appends_one_closed_segment() {
        let tr = t();
        let task = tr.create_task("A", 60, None).unwrap();
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
        tr.create_task("A", 60, None).unwrap();
        let err = tr.create_task("B", 60, None).unwrap_err();
        assert_eq!(err, "another task is active");
    }

    #[test]
    fn pause_resume_accumulates_only_work_segments() {
        let tr = t();
        tr.create_task("A", 100, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        tr.pause("rest").unwrap();
        let paused = tr.current().unwrap().unwrap();
        assert_eq!(paused.status, "paused");
        assert!(paused.net_seconds >= 1, "net {}", paused.net_seconds);
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let still_paused = tr.current().unwrap().unwrap();
        assert_eq!(still_paused.net_seconds, paused.net_seconds);
        tr.resume().unwrap();
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
        tr.create_task("A", 60, None).unwrap();
        let err = tr.pause("bogus").unwrap_err();
        assert!(err.contains("invalid pause reason"));
    }

    #[test]
    fn finish_fixes_totals_and_events() {
        let tr = t();
        let task = tr.create_task("A", 60, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let done = tr.finish().unwrap();
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
        tr.create_task("A", 60, None).unwrap();
        let t2 = tr.abandon().unwrap();
        assert_eq!(t2.status, "abandoned");
        assert!(t2.finished_at.is_some());
    }

    #[test]
    fn list_finished_orders_desc() {
        let tr = t();
        let a = tr.create_task("A", 60, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        tr.finish().unwrap();
        let b = tr.create_task("B", 60, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        tr.finish().unwrap();
        let list = tr.list_finished().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id);
        assert_eq!(list[1].id, a.id);
    }
}
