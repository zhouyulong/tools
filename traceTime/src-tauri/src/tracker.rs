use crate::store;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;

const PAUSE_REASONS: [&str; 4] = ["rest", "disturbance", "switch", "other"];
const TASK_COLUMNS: &str =
    "id, name, est_seconds, status, tag, COALESCE(net_seconds, 0) AS net_seconds, created_at, finished_at";

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

fn elapsed_millis(start: &str, end: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(start).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end).ok()?;
    Some((end - start).num_milliseconds().max(0))
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

    fn net_seconds(conn: &Connection, task_id: &str) -> i64 {
        let current = now();
        let mut stmt = match conn.prepare(
            "SELECT started_at, ended_at FROM segments WHERE task_id = ?1 AND kind = 'work'",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return 0,
        };
        let rows = match stmt.query_map([task_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return 0,
        };
        rows.filter_map(Result::ok)
            .filter_map(|(start, end)| elapsed_millis(&start, end.as_deref().unwrap_or(&current)))
            .sum()
    }

    fn gross_seconds(conn: &Connection, task_id: &str) -> i64 {
        let created_at = conn
            .query_row(
                "SELECT created_at FROM tasks WHERE id = ?1",
                [task_id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        created_at
            .and_then(|created_at| elapsed_millis(&created_at, &now()))
            .unwrap_or(0)
    }

    fn get_task(conn: &Connection, task_id: &str) -> Option<TaskDto> {
        conn.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
            [task_id],
            row_to_task,
        )
        .ok()
    }

    fn active_task(conn: &Connection) -> Option<TaskDto> {
        let id: String = conn
            .query_row(
                "SELECT id FROM tasks WHERE status IN ('pending','running','paused') LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok()?;
        Self::get_task(conn, &id)
    }

    fn require_active(conn: &Connection) -> Result<TaskDto, String> {
        Self::active_task(conn).ok_or_else(|| "no active task".into())
    }

    fn close_open_segment(conn: &Connection, task_id: &str) -> Result<(), String> {
        let n = conn
            .execute(
                "UPDATE segments SET ended_at = ?2
                 WHERE task_id = ?1 AND ended_at IS NULL",
                rusqlite::params![task_id, now()],
            )
            .map_err(|e| e.to_string())?;
        if n > 1 {
            return Err("more than one open segment".into());
        }
        Ok(())
    }

    /// 补齐 net_seconds / gross_seconds 的实时计算，返回给前端
    fn with_live_net(conn: &Connection, mut task: TaskDto) -> TaskDto {
        task.net_seconds = Self::net_seconds(conn, &task.id);
        task
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
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        if Self::active_task(&conn).is_some() {
            return Err("another task is active".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        conn.execute(
            "INSERT INTO tasks (id, name, est_seconds, status, created_at, tag)
             VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
            rusqlite::params![id, name.trim(), est_seconds, ts, tag],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO segments (task_id, started_at, ended_at, kind)
             VALUES (?1, ?2, NULL, 'work')",
            rusqlite::params![id, ts],
        )
        .map_err(|e| e.to_string())?;
        Self::add_event(&conn, &id, "created", None)?;
        Self::add_event(&conn, &id, "started", None)?;
        let task = Self::get_task(&conn, &id).ok_or_else(|| "created task not found".to_string())?;
        Ok(Self::with_live_net(&conn, task))
    }

    pub fn pause(&self, reason: &str) -> Result<TaskDto, String> {
        if !PAUSE_REASONS.contains(&reason) {
            return Err(format!("invalid pause reason: {reason}"));
        }
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let task = Self::require_active(&conn)?;
        if task.status != "running" {
            return Err(format!("cannot pause task in status {}", task.status));
        }
        Self::close_open_segment(&conn, &task.id)?;
        let detail = serde_json::json!({ "reason": reason }).to_string();
        Self::add_event(&conn, &task.id, "paused", Some(&detail))?;
        conn.execute("UPDATE tasks SET status='paused' WHERE id=?1", [&task.id])
            .map_err(|e| e.to_string())?;
        let task = Self::get_task(&conn, &task.id).ok_or_else(|| "paused task not found".to_string())?;
        Ok(Self::with_live_net(&conn, task))
    }

    pub fn resume(&self) -> Result<TaskDto, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let task = Self::require_active(&conn)?;
        if task.status != "paused" {
            return Err(format!("cannot resume task in status {}", task.status));
        }
        conn.execute(
            "INSERT INTO segments (task_id, started_at, ended_at, kind)
             VALUES (?1, ?2, NULL, 'work')",
            rusqlite::params![task.id, now()],
        )
        .map_err(|e| e.to_string())?;
        Self::add_event(&conn, &task.id, "resumed", None)?;
        conn.execute("UPDATE tasks SET status='running' WHERE id=?1", [&task.id])
            .map_err(|e| e.to_string())?;
        let task = Self::get_task(&conn, &task.id).ok_or_else(|| "resumed task not found".to_string())?;
        Ok(Self::with_live_net(&conn, task))
    }

    fn terminate(&self, status: &str, kind: &str) -> Result<TaskDto, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let task = Self::require_active(&conn)?;
        if task.status == "running" {
            Self::close_open_segment(&conn, &task.id)?;
        }
        let net = Self::net_seconds(&conn, &task.id);
        let gross = Self::gross_seconds(&conn, &task.id);
        let finished_at = now();
        conn.execute(
            "UPDATE tasks SET status=?2, finished_at=?3, net_seconds=?4, gross_seconds=?5 WHERE id=?1",
            rusqlite::params![task.id, status, finished_at, net, gross],
        )
        .map_err(|e| e.to_string())?;
        Self::add_event(&conn, &task.id, kind, None)?;
        Self::get_task(&conn, &task.id).ok_or_else(|| "finished task not found".to_string())
    }

    pub fn finish(&self) -> Result<TaskDto, String> {
        self.terminate("done", "finished")
    }

    pub fn abandon(&self) -> Result<TaskDto, String> {
        self.terminate("abandoned", "abandoned")
    }

    pub fn current(&self) -> Result<Option<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        Ok(Self::active_task(&conn).map(|t| Self::with_live_net(&conn, t)))
    }

    pub fn list_finished(&self) -> Result<Vec<TaskDto>, String> {
        let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE status IN ('done','abandoned') ORDER BY finished_at DESC"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_task).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tracker {
        Tracker::new(store::open_in_memory().unwrap())
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
        std::thread::sleep(std::time::Duration::from_millis(50));
        tr.pause("rest").unwrap();
        let paused = tr.current().unwrap().unwrap();
        assert_eq!(paused.status, "paused");
        assert!(paused.net_seconds >= 40, "net {}", paused.net_seconds);
        std::thread::sleep(std::time::Duration::from_millis(60));
        tr.resume().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let running = tr.current().unwrap().unwrap();
        // 第二段至少 40ms；暂停的 60ms 不得计入
        assert!(running.net_seconds >= 80, "net {}", running.net_seconds);
        assert!(running.net_seconds < 130, "net {}", running.net_seconds);
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
        std::thread::sleep(std::time::Duration::from_millis(30));
        let done = tr.finish().unwrap();
        assert_eq!(done.status, "done");
        assert!(done.finished_at.is_some());
        assert!(done.net_seconds >= 25);
        assert!(tr.current().unwrap().is_none()); // 无活动任务返回 None
        // 事件齐全
        let conn = tr.conn.lock().unwrap();
        let kinds: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT kind FROM events WHERE task_id=?1 ORDER BY at, id")
                .unwrap();
            let rows = stmt.query_map([&task.id], |r| r.get::<_, String>(0)).unwrap();
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
