use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    // v1: 建表，与设计文档第 6 节一致
    r#"
    CREATE TABLE tasks (
      id             TEXT PRIMARY KEY,
      name           TEXT NOT NULL,
      est_seconds    INTEGER NOT NULL,
      status         TEXT NOT NULL,
      created_at     TEXT NOT NULL,
      finished_at    TEXT,
      net_seconds    INTEGER,
      gross_seconds  INTEGER,
      parent_task_id TEXT REFERENCES tasks(id),
      tag            TEXT
    );
    CREATE TABLE segments (
      id         INTEGER PRIMARY KEY AUTOINCREMENT,
      task_id    TEXT NOT NULL REFERENCES tasks(id),
      started_at TEXT NOT NULL,
      ended_at   TEXT,             -- NULL 表示进行中的开放段（只增不改原则针对已结束段）
      kind       TEXT NOT NULL DEFAULT 'work'
    );
    CREATE TABLE events (
      id     INTEGER PRIMARY KEY AUTOINCREMENT,
      task_id TEXT NOT NULL REFERENCES tasks(id),
      at     TEXT NOT NULL,
      kind   TEXT NOT NULL,
      detail TEXT
    );
    CREATE TABLE reminder_fired (
      task_id  TEXT NOT NULL REFERENCES tasks(id),
      node     TEXT NOT NULL,
      fired_at TEXT NOT NULL,
      PRIMARY KEY (task_id, node)
    );
    CREATE TABLE settings (
      key   TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
    CREATE INDEX idx_events_task_time ON events(task_id, at);
    CREATE INDEX idx_segments_task ON segments(task_id, started_at);
    "#,
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY);",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_all_tables() {
        let conn = open_in_memory().unwrap();
        for table in ["tasks", "segments", "events", "reminder_fired", "settings"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {table} missing");
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("tt-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");
        open(&db).unwrap();
        open(&db).unwrap(); // 二次打开不报错
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
