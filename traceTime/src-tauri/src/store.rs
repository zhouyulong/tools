use rusqlite::Connection;
use std::path::Path;

pub(crate) const OVERDUE_UPGRADE_CUTOFF_KEY: &str = "reminder_overdue_upgrade_cutoff";

// v9 is applied by Tracker in the startup recovery transaction, not before it.
pub(crate) const RECOVERY_SCHEDULE_MIGRATION: i64 = 9;

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
    // v3: task-level reminder notification mute.
    "ALTER TABLE tasks ADD COLUMN reminder_muted INTEGER NOT NULL DEFAULT 0;",
    // v4: do not backfill past milestones when a task's repeat interval changes.
    "ALTER TABLE tasks ADD COLUMN overdue_interval_changed_at_net INTEGER;",
    // v5: legacy wall-clock upgrade marker, retained for schema compatibility.
    "ALTER TABLE tasks ADD COLUMN overdue_schedule_started_at TEXT;
     UPDATE tasks SET overdue_schedule_started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
     WHERE status IN ('running', 'paused');",
    // v6: freeze the cutover in net seconds so future wall-clock changes cannot move it.
    "ALTER TABLE tasks ADD COLUMN overdue_upgrade_cutoff_net INTEGER;",
    // v7: data-only reset of cutoffs saved by older v6 builds.
    "",
    // v8: repeat the data-only reset for databases that ran the earlier v7.
    "",
];

pub fn try_lock_instance(path: &Path) -> std::io::Result<Option<std::fs::File>> {
    use std::fs::{File, TryLockError};
    // Leave the lock file in place: unlinking it could let another process lock
    // a different inode while this one still holds the original handle.
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

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
            if version == 6 {
                tx.execute(
                    "INSERT INTO settings (key, value)
                     VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    [OVERDUE_UPGRADE_CUTOFF_KEY],
                )?;
            }
            if matches!(version, 6..=8) {
                crate::tracker::Tracker::reset_overdue_schedule_on_upgrade(&tx).map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(std::io::Error::other(e).into())
                })?;
            }
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
    fn instance_lock_blocks_a_second_handle_until_the_owner_exits() {
        let path =
            std::env::temp_dir().join(format!("tracetime-test-{}.lock", uuid::Uuid::new_v4()));
        let original = try_lock_instance(&path).unwrap().unwrap();
        assert!(try_lock_instance(&path).unwrap().is_none());
        drop(original);
        assert!(try_lock_instance(&path).unwrap().is_some());
        std::fs::remove_file(path).unwrap();
    }

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
            8
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
            8
        );
    }

    #[test]
    fn migration_v3_upgrades_v2_and_defaults_existing_tasks_to_unmuted() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute_batch(MIGRATIONS[1]).unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);
          INSERT INTO schema_migrations VALUES (1);
          INSERT INTO schema_migrations VALUES (2);
          INSERT INTO tasks (id,name,est_seconds,status,created_at) VALUES ('old','old',60,'done','2026-11-26T00:00:00Z');").unwrap();
        migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT reminder_muted FROM tasks WHERE id='old'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            8
        );
    }

    fn legacy_overdue_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..3] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);
             INSERT INTO schema_migrations VALUES (1), (2), (3);
             INSERT INTO tasks (id, name, est_seconds, status, created_at)
                 VALUES ('old', 'old', 60, 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-1040 seconds'));
             INSERT INTO segments (task_id, started_at, ended_at, kind)
                 SELECT id, created_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-410 seconds'), 'work' FROM tasks;
             INSERT INTO events (task_id, at, kind)
                 SELECT task_id, started_at, 'started' FROM segments;
             INSERT INTO events (task_id, at, kind)
                 SELECT task_id, ended_at, 'paused' FROM segments;
             INSERT INTO events (task_id, at, kind)
                 VALUES ('old', strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-95 seconds'), 'resumed');
             INSERT INTO reminder_fired (task_id, node, fired_at)
                 SELECT 'old', column1, strftime('%Y-%m-%dT%H:%M:%fZ', at, '+35 seconds')
                 FROM events CROSS JOIN (VALUES ('warn'), ('near'), ('due'), ('overdue:1'))
                 WHERE kind = 'resumed';",
        )
        .unwrap();
        conn
    }

    #[test]
    fn upgrade_skips_legacy_overdue_milestones_but_fires_a_new_one() {
        let tr = crate::tracker::Tracker::new(initialize_connection(legacy_overdue_db()).unwrap());
        // Net time at upgrade is 630s closed + ~95s live. The 660s boundary is old.
        assert_no_reminders(&tr);
        let cutoff = saved_upgrade_cutoff(&tr);
        assert!(cutoff >= 724);

        // Reopening must not take a new snapshot. The old 1260s net boundary
        // is reached before the first complete interval on the reset schedule.
        set_live_work_seconds(&tr, 635);
        migrate(&tr.conn.lock().unwrap()).unwrap();
        assert_eq!(saved_upgrade_cutoff(&tr), cutoff);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, cutoff + 600 - 630);
        assert_next_overdue_repeats_once(&tr, cutoff + 600 - 60);
    }

    #[test]
    fn recovery_reanchors_a_running_tasks_upgraded_schedule_at_closed_work() {
        let tr = crate::tracker::Tracker::new(initialize_connection(legacy_overdue_db()).unwrap());
        assert!(saved_upgrade_cutoff(&tr) >= 724);
        tr.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE tasks SET overdue_interval_changed_at_net=5000 WHERE id='old'",
                [],
            )
            .unwrap();
        let recovered = tr.recover_running_tasks(chrono::Utc::now()).unwrap();
        assert_eq!(recovered[0].net_seconds, 630);
        assert_eq!(saved_upgrade_cutoff(&tr), 630);
        tr.resume("old").unwrap();
        set_live_work_seconds(&tr, 599);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, 600);
        assert_next_overdue_repeats_once(&tr, 1170);

        // A later episode must still reset the cutoff after the backfill has been marked.
        tr.conn.lock().unwrap().execute(
            "UPDATE tasks SET overdue_upgrade_cutoff_net=5000, overdue_interval_changed_at_net=5000 WHERE id='old'", [],
        ).unwrap();
        let recovered = tr.recover_running_tasks(chrono::Utc::now()).unwrap();
        assert_eq!(recovered[0].net_seconds, 630);
        assert_eq!(saved_upgrade_cutoff(&tr), 630);
        assert!(
            tr.recover_running_tasks(chrono::Utc::now())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn recovery_backfills_v8_paused_anomalies_once_without_rewriting_history() {
        let conn = initialize_connection(legacy_overdue_db()).unwrap();
        conn.execute_batch(
            "UPDATE tasks SET status='paused', overdue_upgrade_cutoff_net=777660,
                 overdue_interval_changed_at_net=432000 WHERE id='old';
             INSERT INTO events (task_id, at, kind, detail) VALUES
                 ('old', '2026-11-26T00:00:00Z', 'paused', '{\"reason\":\"anomaly\"}'),
                 ('old', '2026-11-26T00:00:00Z', 'anomaly_recovered', NULL);
             INSERT INTO segments (task_id, started_at, ended_at, kind)
                 VALUES ('old', '2026-11-25T23:59:00Z', '2026-11-26T00:00:00Z', 'anomaly');
             INSERT INTO tasks (id, name, est_seconds, status, created_at,
                                overdue_upgrade_cutoff_net, overdue_interval_changed_at_net)
                 VALUES ('done', 'done', 60, 'done', '2026-01-01T00:00:00Z', 777660, 432000),
                        ('normal', 'normal', 60, 'paused', '2026-01-01T00:00:00Z', 45, 80);
             INSERT INTO events (task_id, at, kind) VALUES
                 ('done', '2026-01-01T00:00:00Z', 'anomaly_recovered');
             CREATE TRIGGER protect_history BEFORE UPDATE ON tasks WHEN OLD.status='done'
                 BEGIN SELECT RAISE(ABORT, 'terminal task rewritten'); END;
             CREATE TRIGGER protect_legacy_fires BEFORE UPDATE ON reminder_fired
                 BEGIN SELECT RAISE(ABORT, 'legacy reminder rewritten'); END;
             CREATE TRIGGER reject_backfill BEFORE INSERT ON schema_migrations WHEN NEW.version=9
                 BEGIN SELECT RAISE(ABORT, 'backfill marker rejected'); END;",
        )
        .unwrap();
        let tr = crate::tracker::Tracker::new(conn);
        let snapshot = |id: &str| -> (i64, Option<i64>) {
            tr.conn.lock().unwrap().query_row(
                "SELECT overdue_upgrade_cutoff_net, overdue_interval_changed_at_net FROM tasks WHERE id=?1",
                [id], |row| Ok((row.get(0)?, row.get(1)?)),
            ).unwrap()
        };
        assert!(tr.recover_running_tasks(chrono::Utc::now()).is_err());
        assert_eq!(snapshot("old"), (777660, Some(432000)));
        tr.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_backfill;")
            .unwrap();
        assert!(
            tr.recover_running_tasks(chrono::Utc::now())
                .unwrap()
                .is_empty()
        );
        assert_eq!(snapshot("old"), (630, None));
        assert_eq!(snapshot("done"), (777660, Some(432000)));
        assert_eq!(snapshot("normal"), (45, Some(80)));
        let detail = tr.task_detail("old").unwrap();
        assert_eq!(detail.segments.len(), 2);
        assert_eq!(detail.events.len(), 5);
        assert_eq!(
            tr.conn
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM reminder_fired WHERE task_id='old'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            4
        );

        tr.set_setting("reminder_overdue_interval_minutes", "2")
            .unwrap();
        assert!(
            tr.recover_running_tasks(chrono::Utc::now())
                .unwrap()
                .is_empty()
        );
        assert_eq!(snapshot("old"), (630, Some(630)));
        tr.resume("old").unwrap();
        set_live_work_seconds(&tr, 119);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, 120);
        assert_next_overdue_repeats_once(&tr, 690);
    }

    #[test]
    fn v9_preserves_a_valid_schedule_after_an_anomaly_and_normal_pause() {
        for (cutoff, interval_boundary) in [(None, 650), (Some(60), 655)] {
            let tr = paused_after_anomaly(cutoff, interval_boundary);
            assert!(
                tr.recover_running_tasks(chrono::Utc::now())
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(paused_schedule(&tr), (cutoff, Some(interval_boundary)));
            tr.resume("old").unwrap();
            set_live_work_seconds(&tr, 9);
            assert_no_reminders(&tr);
            set_live_work_seconds(&tr, 10);
            assert_next_overdue_repeats_once(&tr, 600); // 660 net at the old milestone.
        }
    }

    #[test]
    fn v9_repairs_a_stale_interval_boundary_that_skips_the_existing_repeat() {
        let tr = paused_after_anomaly(Some(60), 661);
        assert!(
            tr.recover_running_tasks(chrono::Utc::now())
                .unwrap()
                .is_empty()
        );
        assert_eq!(paused_schedule(&tr), (Some(650), None));
        tr.resume("old").unwrap();
        set_live_work_seconds(&tr, 599);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, 600);
        assert_next_overdue_repeats_once(&tr, 1190); // 1250 net at the repaired milestone.
    }

    fn paused_after_anomaly(
        cutoff: Option<i64>,
        interval_boundary: i64,
    ) -> crate::tracker::Tracker {
        let conn = initialize_connection(legacy_overdue_db()).unwrap();
        conn.execute_batch(
            "INSERT INTO events (task_id, at, kind, detail) VALUES
                 ('old', '2026-11-26T00:00:00Z', 'paused', '{\"reason\":\"anomaly\"}'),
                 ('old', '2026-11-26T00:00:00Z', 'anomaly_recovered', NULL),
                 ('old', '2026-11-26T00:01:00Z', 'resumed', NULL),
                 ('old', '2026-11-26T00:01:20Z', 'paused', '{\"reason\":\"rest\"}');
             INSERT INTO segments (task_id, started_at, ended_at, kind)
                 VALUES ('old', '2026-11-25T23:59:00Z', '2026-11-26T00:00:00Z', 'anomaly'),
                        ('old', '2026-11-26T00:01:00Z', '2026-11-26T00:01:20Z', 'work');",
        )
        .unwrap();
        conn.execute(
            "UPDATE tasks SET status='paused', overdue_upgrade_cutoff_net=?1,
                overdue_interval_changed_at_net=?2 WHERE id='old'",
            rusqlite::params![cutoff, interval_boundary],
        )
        .unwrap();
        let tr = crate::tracker::Tracker::new(conn);
        assert_eq!(tr.task_detail("old").unwrap().task.net_seconds, 650);
        tr
    }

    fn paused_schedule(tr: &crate::tracker::Tracker) -> (Option<i64>, Option<i64>) {
        tr.conn.lock().unwrap().query_row(
            "SELECT overdue_upgrade_cutoff_net, overdue_interval_changed_at_net FROM tasks WHERE id='old'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap()
    }

    fn simulate_rollback_pause_and_resume(conn: &Connection) {
        // Move the original work and fire into the past to simulate the clock
        // falling back. The next pause closes only five live seconds, then the
        // task resumes at the corrected clock.
        conn.execute_batch(
            "UPDATE segments SET started_at = strftime('%Y-%m-%dT%H:%M:%fZ', started_at, '-120 seconds'),
                 ended_at = strftime('%Y-%m-%dT%H:%M:%fZ', ended_at, '-120 seconds');
             UPDATE events SET at = strftime('%Y-%m-%dT%H:%M:%fZ', at, '-120 seconds');
             UPDATE reminder_fired SET fired_at = strftime('%Y-%m-%dT%H:%M:%fZ', fired_at, '-120 seconds');
             INSERT INTO segments (task_id, started_at, ended_at, kind)
                 SELECT task_id, at, strftime('%Y-%m-%dT%H:%M:%fZ', at, '+5 seconds'), 'work'
                 FROM events WHERE kind = 'resumed';
             INSERT INTO events (task_id, at, kind)
                 SELECT 'old', ended_at, 'paused' FROM segments ORDER BY id DESC LIMIT 1;
             INSERT INTO events (task_id, at, kind)
                 VALUES ('old', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'resumed');",
        ).unwrap();
    }

    fn advance_resumed_task(conn: &Connection) {
        conn.execute_batch(
            "UPDATE events SET at = strftime('%Y-%m-%dT%H:%M:%fZ', at, '-40 seconds')
             WHERE id = (SELECT MAX(id) FROM events WHERE kind = 'resumed');",
        )
        .unwrap();
    }

    fn assert_no_reminders(tr: &crate::tracker::Tracker) -> i64 {
        let checked = tr.check_reminders("old").unwrap();
        assert!(
            checked.reminders.is_empty(),
            "replayed {:?}",
            checked.reminders
        );
        assert!(
            tr.task_detail("old")
                .unwrap()
                .events
                .iter()
                .all(|e| e.kind != "reminder_fired")
        );
        checked.net_seconds
    }

    fn saved_upgrade_cutoff(tr: &crate::tracker::Tracker) -> i64 {
        tr.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT overdue_upgrade_cutoff_net FROM tasks WHERE id='old'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn set_live_work_seconds(tr: &crate::tracker::Tracker, seconds: i64) {
        let resumed_at = chrono::Utc::now() - chrono::TimeDelta::seconds(seconds);
        tr.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET at = ?1
             WHERE id = (SELECT MAX(id) FROM events WHERE task_id='old' AND kind='resumed')",
                [resumed_at.to_rfc3339()],
            )
            .unwrap();
    }

    fn assert_next_overdue_repeats_once(tr: &crate::tracker::Tracker, overtime: i64) {
        let node = format!("overdue:at:{overtime}");
        let next = tr.check_reminders("old").unwrap();
        assert_eq!(
            next.reminders
                .iter()
                .map(|r| r.node.as_str())
                .collect::<Vec<_>>(),
            [node.as_str()]
        );
        assert!(tr.check_reminders("old").unwrap().reminders.is_empty());
        let detail = tr.task_detail("old").unwrap();
        let events = detail
            .events
            .iter()
            .filter(|e| e.kind == "reminder_fired")
            .map(|e| {
                serde_json::from_str::<serde_json::Value>(e.detail.as_deref().unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            [serde_json::json!({"node": node, "level": "overdue"})]
        );
    }

    #[test]
    fn direct_v3_upgrade_resets_after_rollback_pause_and_resume() {
        let conn = legacy_overdue_db();
        // The fire saw 665 net seconds, but only 635 are closed at migration.
        simulate_rollback_pause_and_resume(&conn);
        let tr = crate::tracker::Tracker::new(initialize_connection(conn).unwrap());
        let cutoff = saved_upgrade_cutoff(&tr);
        assert!((635..665).contains(&cutoff));
        set_live_work_seconds(&tr, 25);
        assert!(assert_no_reminders(&tr) >= 660);
        set_live_work_seconds(&tr, cutoff + 590 - 635);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, cutoff + 600 - 635);
        assert_next_overdue_repeats_once(&tr, cutoff + 600 - 60);
    }

    fn legacy_overdue_db_with_long_pause() -> Connection {
        let conn = legacy_overdue_db();
        for sql in &MIGRATIONS[3..5] {
            conn.execute_batch(sql).unwrap();
        }
        // Ten minutes of work, nine days paused, then 65 seconds of work before
        // pausing again. The v5 marker and legacy fire precede the final pause.
        conn.execute_batch(
            "INSERT INTO schema_migrations VALUES (4), (5);
             UPDATE tasks SET created_at = '2026-01-01T00:00:00Z',
                 overdue_schedule_started_at = '2026-01-10T00:01:00Z';
             UPDATE segments SET started_at = '2026-01-01T00:00:00Z', ended_at = '2026-01-01T01:10:00+01:00';
             UPDATE events SET at = CASE kind
                 WHEN 'started' THEN '2026-01-01T00:00:00Z'
                 WHEN 'paused' THEN '2026-01-01T01:10:00+01:00'
                 WHEN 'resumed' THEN '2026-01-10T02:00:00+02:00' END;
             UPDATE reminder_fired SET fired_at = '2026-01-09T19:01:00-05:00';
             INSERT INTO segments (task_id, started_at, ended_at, kind)
                 VALUES ('old', '2026-01-10T02:00:00+02:00', '2026-01-10T00:01:05Z', 'work');
             INSERT INTO events (task_id, at, kind)
                 VALUES ('old', '2026-01-10T00:01:05Z', 'paused');
             UPDATE tasks SET status = 'paused' WHERE id = 'old';",
        ).unwrap();
        conn
    }

    #[test]
    fn v5_backfill_does_not_count_a_long_ordinary_pause_as_work() {
        let tr = crate::tracker::Tracker::new(
            initialize_connection(legacy_overdue_db_with_long_pause()).unwrap(),
        );
        assert_eq!(saved_upgrade_cutoff(&tr), 665);
        tr.resume("old").unwrap();
        set_live_work_seconds(&tr, 590);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, 600);
        assert_next_overdue_repeats_once(&tr, 1205);
    }

    #[test]
    fn v7_and_v8_reset_inflated_v6_cutoff_after_a_clock_correction() {
        let conn = legacy_overdue_db_with_long_pause();
        conn.execute_batch(MIGRATIONS[5]).unwrap();
        conn.execute_batch(
            "INSERT INTO schema_migrations VALUES (6);
             INSERT INTO settings (key, value)
                 VALUES ('reminder_overdue_upgrade_cutoff', '2026-01-11T00:00:00Z');
             UPDATE tasks SET overdue_upgrade_cutoff_net = 777660, status = 'running',
                 overdue_interval_changed_at_net = 432000,
                 overdue_schedule_started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+9 days')
                 WHERE id = 'old';
             INSERT INTO events (task_id, at, kind)
                 VALUES ('old', strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+4 days'), 'resumed');
             INSERT INTO tasks (id, name, est_seconds, status, created_at,
                                overdue_schedule_started_at, overdue_upgrade_cutoff_net)
                 VALUES ('done', 'done', 60, 'done', '2026-01-01T00:00:00Z',
                         '2026-01-10T00:01:00Z', 777660),
                        ('newer', 'newer', 60, 'paused', '2026-01-01T00:00:00Z', NULL, NULL);
             INSERT INTO segments (task_id, started_at, ended_at, kind)
                 VALUES ('newer', '2026-01-01T00:00:00Z', '2026-01-01T00:00:45Z', 'work');",
        )
        .unwrap();

        let cutoff = |id: &str| -> i64 {
            conn.query_row(
                "SELECT overdue_upgrade_cutoff_net FROM tasks WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
        };
        // A failure to record v7 must also roll back its data updates.
        conn.execute_batch(
            "CREATE TRIGGER reject_v7 BEFORE INSERT ON schema_migrations WHEN NEW.version = 7
             BEGIN SELECT RAISE(ABORT, 'injected v7 failure'); END;",
        )
        .unwrap();
        assert!(
            migrate(&conn)
                .unwrap_err()
                .to_string()
                .contains("injected v7 failure")
        );
        assert_eq!(cutoff("old"), 777660);
        conn.execute_batch("DROP TRIGGER reject_v7;").unwrap();

        migrate(&conn).unwrap();
        // After the clock correction, the live run contributes zero, not five days.
        assert_eq!(cutoff("old"), 665);
        assert_eq!(cutoff("newer"), 45);
        assert_eq!(cutoff("done"), 777660);
        assert_eq!(
            conn.query_row(
                "SELECT fired_at FROM reminder_fired WHERE task_id='old' AND node='overdue:1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "2026-01-09T19:01:00-05:00"
        );
        assert_eq!(
            conn.query_row(
                "SELECT value FROM settings WHERE key=?1",
                [OVERDUE_UPGRADE_CUTOFF_KEY],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "2026-01-11T00:00:00Z"
        );
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            8
        );
        conn.execute_batch(
            "CREATE TRIGGER reject_cutoff_rewrite BEFORE UPDATE OF overdue_upgrade_cutoff_net ON tasks
             BEGIN SELECT RAISE(ABORT, 'schedule migration reapplied'); END;",
        ).unwrap();
        migrate(&conn).unwrap();

        let tr = crate::tracker::Tracker::new(conn);
        set_live_work_seconds(&tr, 590);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, 600);
        assert_next_overdue_repeats_once(&tr, 1205);
    }

    #[test]
    fn v8_resets_a_schedule_saved_by_the_earlier_v7_build() {
        let conn = legacy_overdue_db_with_long_pause();
        conn.execute_batch(MIGRATIONS[5]).unwrap();
        conn.execute_batch(
            "INSERT INTO schema_migrations VALUES (6), (7);
             INSERT INTO settings (key, value)
                 VALUES ('reminder_overdue_upgrade_cutoff', '2026-01-11T00:00:00Z');
             UPDATE tasks SET overdue_upgrade_cutoff_net = 777660,
                 overdue_interval_changed_at_net = 432000 WHERE id = 'old';
             INSERT INTO tasks (id, name, est_seconds, status, created_at,
                                overdue_upgrade_cutoff_net)
                 VALUES ('done', 'done', 60, 'done', '2026-01-01T00:00:00Z', 777660);",
        )
        .unwrap();
        let snapshot = || {
            conn.query_row(
                "SELECT (SELECT MAX(version) FROM schema_migrations),
                        (SELECT overdue_upgrade_cutoff_net FROM tasks WHERE id='old'),
                        (SELECT overdue_upgrade_cutoff_net FROM tasks WHERE id='done'),
                        (SELECT fired_at FROM reminder_fired WHERE task_id='old' AND node='overdue:1')",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
            ).unwrap()
        };
        let before = snapshot();
        conn.execute_batch(
            "CREATE TRIGGER reject_v8 BEFORE INSERT ON schema_migrations WHEN NEW.version = 8
             BEGIN SELECT RAISE(ABORT, 'injected v8 failure'); END;",
        )
        .unwrap();
        assert!(
            migrate(&conn)
                .unwrap_err()
                .to_string()
                .contains("injected v8 failure")
        );
        assert_eq!(snapshot(), before);
        conn.execute_batch("DROP TRIGGER reject_v8;").unwrap();

        migrate(&conn).unwrap();
        assert_eq!(snapshot(), (8, 665, before.2, before.3));
        conn.execute_batch(
            "CREATE TRIGGER reject_v8_reapply BEFORE UPDATE OF overdue_upgrade_cutoff_net ON tasks
             BEGIN SELECT RAISE(ABORT, 'v8 reapplied'); END;",
        )
        .unwrap();
        migrate(&conn).unwrap();

        let tr = crate::tracker::Tracker::new(conn);
        assert_eq!(saved_upgrade_cutoff(&tr), 665);
        tr.resume("old").unwrap();
        set_live_work_seconds(&tr, 590);
        assert_no_reminders(&tr);
        set_live_work_seconds(&tr, 600);
        assert_next_overdue_repeats_once(&tr, 1205);
    }

    #[test]
    fn upgrade_cutoff_survives_clock_rollback_and_pause_resume() {
        let tr = crate::tracker::Tracker::new(initialize_connection(legacy_overdue_db()).unwrap());
        assert!(tr.check_reminders("old").unwrap().reminders.is_empty());
        let cutoff = tr.get_setting(OVERDUE_UPGRADE_CUTOFF_KEY).unwrap().unwrap();
        assert!(cutoff.ends_with('Z'));
        {
            let conn = tr.conn.lock().unwrap();
            simulate_rollback_pause_and_resume(&conn);
            migrate(&conn).unwrap();
        }
        assert!(tr.current().unwrap().unwrap().net_seconds < 660);
        // Changing the interval at a lower net time must not replace the upgrade cutoff.
        tr.set_setting(crate::reminder::OVERDUE_INTERVAL_KEY, "5")
            .unwrap();
        tr.set_setting(crate::reminder::OVERDUE_INTERVAL_KEY, "10")
            .unwrap();
        advance_resumed_task(&tr.conn.lock().unwrap());
        assert_no_reminders(&tr);
        assert_eq!(
            tr.get_setting(OVERDUE_UPGRADE_CUTOFF_KEY).unwrap(),
            Some(cutoff)
        );
    }

    #[test]
    fn v5_backfill_is_safe_even_after_rollback_already_shortened_the_work_history() {
        let conn = legacy_overdue_db();
        for sql in &MIGRATIONS[3..5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch("INSERT INTO schema_migrations VALUES (4), (5);")
            .unwrap();
        simulate_rollback_pause_and_resume(&conn);
        let tr = crate::tracker::Tracker::new(initialize_connection(conn).unwrap());
        assert!(tr.current().unwrap().unwrap().net_seconds < 660);
        assert!((635..665).contains(&saved_upgrade_cutoff(&tr)));
        advance_resumed_task(&tr.conn.lock().unwrap());
        assert_no_reminders(&tr);
        assert!(
            tr.get_setting(OVERDUE_UPGRADE_CUTOFF_KEY)
                .unwrap()
                .is_some()
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
