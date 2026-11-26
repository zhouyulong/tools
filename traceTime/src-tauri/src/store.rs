use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    // v1: 建表，与设计文档第 6 节一致。IF NOT EXISTS 允许恢复旧版本中断留下的部分建表。
    r#"
    CREATE TABLE IF NOT EXISTS tasks (
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
    CREATE TABLE IF NOT EXISTS segments (
      id         INTEGER PRIMARY KEY AUTOINCREMENT,
      task_id    TEXT NOT NULL REFERENCES tasks(id),
      started_at TEXT NOT NULL,
      ended_at   TEXT,             -- NULL 表示进行中的开放段（只增不改原则针对已结束段）
      kind       TEXT NOT NULL DEFAULT 'work'
    );
    CREATE TABLE IF NOT EXISTS events (
      id     INTEGER PRIMARY KEY AUTOINCREMENT,
      task_id TEXT NOT NULL REFERENCES tasks(id),
      at     TEXT NOT NULL,
      kind   TEXT NOT NULL,
      detail TEXT
    );
    CREATE TABLE IF NOT EXISTS reminder_fired (
      task_id  TEXT NOT NULL REFERENCES tasks(id),
      node     TEXT NOT NULL,
      fired_at TEXT NOT NULL,
      PRIMARY KEY (task_id, node)
    );
    CREATE TABLE IF NOT EXISTS settings (
      key   TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_events_task_time ON events(task_id, at);
    CREATE INDEX IF NOT EXISTS idx_segments_task ON segments(task_id, started_at);
    "#,
    // v2: visibility only; preserve all trajectory records.
    "ALTER TABLE tasks ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;",
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    initialize_connection(Connection::open(path)?)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    initialize_connection(Connection::open_in_memory()?)
}

fn initialize_connection(conn: Connection) -> rusqlite::Result<Connection> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
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
            // Schema changes and their version marker must commit or roll back together.
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
            tx.commit()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_recovers_partial_unversioned_schema() {
        let dir = std::env::temp_dir().join(format!("tt-partial-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");
        {
            let conn = Connection::open(&db).unwrap();
            // Simulate a pre-fix crash after the first v1 statement was committed.
            let first_statement = MIGRATIONS[0].split_once(';').unwrap().0;
            conn.execute_batch(&format!("{first_statement};")).unwrap();
            conn.execute(
                "INSERT INTO tasks (id, name, est_seconds, status, created_at)
                          VALUES ('existing', 'keep me', 60, 'done', '2026-11-26T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let conn = open(&db).unwrap();
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row("SELECT name FROM tasks WHERE id='existing'", [], |r| r
                .get::<_, String>(
                0
            ))
            .unwrap(),
            "keep me"
        );
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('ready', 'yes')",
            [],
        )
        .unwrap();
        drop(conn);
        open(&db).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn migration_rolls_back_sql_if_version_insert_fails() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);
             CREATE TRIGGER reject_version BEFORE INSERT ON schema_migrations
             BEGIN SELECT RAISE(ABORT, 'injected migration failure'); END;",
        )
        .unwrap();
        assert!(migrate(&conn).is_err());
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tasks'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0, "failed migration must not commit its schema");
        conn.execute_batch("DROP TRIGGER reject_version;").unwrap();
        migrate(&conn).unwrap();
    }

    #[test]
    fn migration_v2_upgrades_v1_and_defaults_existing_tasks_to_visible() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);
          INSERT INTO schema_migrations VALUES (1);
          INSERT INTO tasks (id,name,est_seconds,status,created_at) VALUES ('old','old',60,'done','2026-11-26T00:00:00Z');").unwrap();
        migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT hidden FROM tasks WHERE id='old'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

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
    fn foreign_keys_reject_orphan_segments_and_events() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        let conn = initialize_connection(conn).unwrap();

        let segment_err = conn
            .execute(
                "INSERT INTO segments (task_id, started_at) VALUES (?1, ?2)",
                ["missing-task", "2026-11-26T00:00:00Z"],
            )
            .unwrap_err();
        assert!(
            segment_err
                .to_string()
                .contains("FOREIGN KEY constraint failed"),
            "unexpected segment error: {segment_err}"
        );

        let event_err = conn
            .execute(
                "INSERT INTO events (task_id, at, kind) VALUES (?1, ?2, ?3)",
                ["missing-task", "2026-11-26T00:00:00Z", "pause"],
            )
            .unwrap_err();
        assert!(
            event_err
                .to_string()
                .contains("FOREIGN KEY constraint failed"),
            "unexpected event error: {event_err}"
        );
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
