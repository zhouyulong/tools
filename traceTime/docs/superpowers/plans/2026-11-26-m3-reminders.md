# traceTime M3（渐进提醒与设置）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付 75% / 90% / 100% / 超时周期提醒、任务级超时通知静音、全局提醒设置页与系统通知。

**Architecture:** Rust 继续作为状态与判断的唯一事实来源：`ReminderEngine` 只做纯判断，`Tracker::check_reminders` 在事务内读取配置、判定、去重并写入 `reminder_fired`。前端按返回配置持续渲染当前提醒等级，收到新触发事件后播放短时动画、音效，并请求后端发送系统通知。

**Tech Stack:** Tauri 2、Svelte 5、TypeScript、rusqlite、tauri-plugin-notification。

**Spec:** `docs/2026-11-26-tracetime-design.md`（第 3.1/8 节）

## Global Constraints

- 默认节点：`warn=75%`、`near=90%`、`due=100%`、超时重复间隔 `10` 分钟；超时后任务保持 running/paused，不自动结束。
- 每个普通节点对每个任务只触发一次；超时节点按 `floor(超时秒数 / 周期秒数)` 分桶触发，节点名为 `overdue:<bucket>`。
- 静音只关闭超时系统通知；due/overdue 的视觉状态保留。
- 配置保存在 `settings` KV，布尔值为 `"true"/"false"`，百分比为整数字符串，超时周期为分钟整数字符串。
- 后端读取配置时必须应用默认值、范围校验和 `warn <= near <= due` 归一化；前端设置页在保存前也校验。
- 段与事件只追加；提醒判定写路径事务化；除 `QueryReturnedNoRows` 外不得把数据库错误当作不存在。
- 系统通知由 Rust 的 notification plugin 发送；前端不直接实现通知逻辑。
- 每个实现任务都要运行 `cd src-tauri && cargo test`；涉及前端时运行 `pnpm check`、`pnpm test`、`pnpm build`。

---

### Task 1: 后端提醒判定、去重与任务静音

**Files:**
- Create: `src-tauri/src/reminder.rs`
- Modify: `src-tauri/src/store.rs`
- Modify: `src-tauri/src/tracker.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/reminder.rs` 与 `src-tauri/src/tracker.rs` 内 tests

**Interfaces:**
- Consumes: 现有 `Tracker`、`TaskDto`、`settings` KV、`reminder_fired` 表。
- Produces:
  - `ReminderConfig`：`reminder_enabled`, `warn_enabled`, `warn_percent`, `near_enabled`, `near_percent`, `due_enabled`, `due_percent`, `overdue_enabled`, `overdue_notify`, `overdue_interval_minutes`, `warn_sound`, `near_sound`, `due_notify`。
  - `due_reminders(net_seconds: i64, config: &ReminderConfig) -> Vec<DueReminder>`。
  - `Tracker::reminder_config() -> Result<ReminderConfigDto, String>`。
  - `Tracker::check_reminders(expected_task_id: &str) -> Result<ReminderCheckDto, String>`。
  - `Tracker::set_reminder_muted(expected_task_id: &str, muted: bool) -> Result<TaskDto, String>`。
  - IPC commands: `check_reminders`, `set_reminder_muted`, `send_reminder_notification`。

- [ ] **Step 1: 先写失败测试**

在 `reminder.rs` tests 中覆盖：

```rust
#[test]
fn default_config_maps_all_four_stages() {
    let config = ReminderConfig::default();
    let due = due_reminders(46, &config); // 75% of 60s
    assert_eq!(due.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(), vec!["warn"]);
    let due = due_reminders(55, &config);
    assert_eq!(due.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(), vec!["warn", "near"]);
    let due = due_reminders(60, &config);
    assert_eq!(due.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(), vec!["warn", "near", "due"]);
}

#[test]
fn overdue_uses_minute_buckets_and_respects_mute_for_notify() {
    let config = ReminderConfig { overdue_interval_minutes: 10, ..ReminderConfig::default() };
    let due = due_reminders(1260, &config); // 20 minutes overdue on a 60s estimate
    assert_eq!(due, vec![DueReminder {
        node: "overdue:2".into(),
        level: ReminderLevel::Overdue,
        title: "任务已超时".into(),
        body: "已超过预计时长 20 分钟".into(),
        notify: true,
        sound: false,
    }]);
}
```

在 `tracker.rs` tests 中覆盖：

```rust
#[test]
fn check_reminders_persists_once_and_repeats_overdue_by_bucket() {
    let tr = t();
    let task = tr.create_task("A", 60, None, None).unwrap();
    {
        let conn = tr.conn.lock().unwrap();
        let past = (chrono::Utc::now() - chrono::Duration::seconds(80)).to_rfc3339();
        conn.execute("UPDATE events SET at=?1 WHERE task_id=?2 AND kind='started'", [&past, &task.id]).unwrap();
    }
    let first = tr.check_reminders(&task.id).unwrap();
    assert_eq!(first.reminders.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(), vec!["warn", "near", "due"]);
    let second = tr.check_reminders(&task.id).unwrap();
    assert!(second.reminders.is_empty());

    {
        let conn = tr.conn.lock().unwrap();
        let past = (chrono::Utc::now() - chrono::Duration::seconds(1300)).to_rfc3339();
        conn.execute("UPDATE events SET at=?1 WHERE task_id=?2 AND kind='started'", [&past, &task.id]).unwrap();
    }
    let third = tr.check_reminders(&task.id).unwrap();
    assert_eq!(third.reminders.iter().map(|x| x.node.as_str()).collect::<Vec<_>>(), vec!["overdue:2"]);
}

#[test]
fn reminder_mute_blocks_only_notifications_and_preserves_visual_config() {
    let tr = t();
    let task = tr.create_task("A", 3600, None, None).unwrap();
    let muted = tr.set_reminder_muted(&task.id, true).unwrap();
    assert!(muted.reminder_muted);
    let checked = tr.check_reminders(&task.id).unwrap();
    assert_eq!(checked.config.reminder_enabled, true);
    assert!(checked.reminders.iter().all(|item| !item.notify));
}
```

运行 `cd src-tauri && cargo test`，确认因接口不存在而失败。

- [ ] **Step 2: 增加任务静音列**

在 `store.rs` 追加 v3 migration：

```sql
ALTER TABLE tasks ADD COLUMN reminder_muted INTEGER NOT NULL DEFAULT 0;
```

同步更新：
- `TASK_COLUMNS` 增加 `reminder_muted`；
- `TaskDto` 增加 `reminder_muted: bool`；
- `row_to_task` 将 `i64` 转成 `!= 0`；
- migration 测试确认旧库升级后默认 `false`。

- [ ] **Step 3: 实现纯判定与 Tracker 集成**

`reminder.rs` 要点：
- `ReminderConfig::from_settings(get: impl Fn(&str) -> Result<Option<String>, String>) -> Result<Self, String>`；
- 默认值：`true,true,75,true,90,true,100,true,true,10,false,false,true`；
- 百分比 clamp `1..=1000`，周期分钟 clamp `1..=1440`；
- 排序为 `warn_percent <= near_percent <= due_percent`，配置顺序颠倒时保持相对开关并归一化数值；
- `due_reminders` 只输出尚未判定的候选节点；去重由 Tracker 负责；
- `warn/near/due` 节点分别叫 `warn/near/due`；超时候选节点叫 `overdue:<bucket>`，仅当 `net >= due_percent * est / 100` 且超出周期时间后输出。

`Tracker` 要点：
- `reminder_config` 从当前设置构造 `ReminderConfigDto`；
- `check_reminders` 只处理 running 任务；paused 返回配置和空列表；
- 在事务中调用 `closed + live` 净耗时；
- 对 `warn/near/due` 使用固定节点，对超时候选使用 `overdue:<bucket>`；
- 用 `INSERT ... ON CONFLICT(task_id,node) DO NOTHING` 检查受影响行数，只有新插入才加入返回列表；
- 通知字段：`due_notify && !reminder_muted`、`overdue_notify && !reminder_muted`；声音字段来自 `warn_sound/near_sound`；
- `set_reminder_muted` 校验任务存在且非终态，更新 v3 字段并返回最新 `TaskDto`；
- 删除未被引用的 `setting_or`，消除 dead code warning。

- [ ] **Step 4: 暴露命令**

`commands.rs` 增加 `check_reminders`、`set_reminder_muted`；`send_reminder_notification` 接收 `AppHandle`、`title`、`body`，用 notification plugin 发送；所有错误转成 `String`。

`main.rs`：
- 注册 `reminder` 模块；
- 注册三个命令；
- 注册 `tauri_plugin_notification::init()`。

- [ ] **Step 5: 配置依赖与权限**

- `src-tauri/Cargo.toml` 增加 `tauri-plugin-notification = "2"`；
- `src-tauri/capabilities/default.json` 的 `permissions` 增加 `"notification:default"`；
- `windows` 增加 `"settings"`，为 Task 3 预留。

- [ ] **Step 6: 验证并提交**

Run: `cd src-tauri && cargo test && cd .. && pnpm check`
Expected: Rust 新旧测试全绿；前端类型检查通过；无 `setting_or is never used` warning。

```bash
git add docs/superpowers/plans/2026-11-26-m3-reminders.md docs/2026-11-26-tracetime-design.md src-tauri
git commit -m "feat(reminders): backend stages, dedupe and task mute"
```

---

### Task 2: 前端持续提醒状态、动画、音效与通知

**Files:**
- Modify: `src/lib/api.ts`
- Create: `src/lib/reminder.ts`
- Create: `tests/reminder.test.ts`
- Modify: `src/components/WidgetView.svelte`
- Modify: `src/components/WidgetRunning.svelte`
- Test: `tests/reminder.test.ts`

**Interfaces:**
- Consumes Task 1 的 `ReminderCheckDto`, `ReminderDto`, `ReminderConfigDto`, `check_reminders`, `set_reminder_muted`, `send_reminder_notification`。
- Produces:
  - `type ReminderLevel = 'none' | 'warn' | 'near' | 'due' | 'overdue'`
  - `reminderLevel(netSeconds, estSeconds, config): ReminderLevel`
  - `reminderPulse(activeLevel): 'none' | 'gentle' | 'strong'`
  - `playReminderChime(kind: 'soft' | 'urgent')`

- [ ] **Step 1: 先写失败测试**

`tests/reminder.test.ts` 至少覆盖：

```ts
test('reminderLevel maps progressive thresholds and overdue', async () => {
  const { reminderLevel } = await import('../src/lib/reminder.ts')
  const config = { reminderEnabled: true, warnPercent: 75, nearPercent: 90, duePercent: 100, overdueIntervalMinutes: 10 }
  assert.equal(reminderLevel(0, 60, config), 'none')
  assert.equal(reminderLevel(45, 60, config), 'warn')
  assert.equal(reminderLevel(55, 60, config), 'near')
  assert.equal(reminderLevel(60, 60, config), 'due')
  assert.equal(reminderLevel(61, 60, config), 'overdue')
  assert.equal(reminderLevel(50, 60, { ...config, reminderEnabled: false }), 'none')
})

test('reminderPulse keeps transient animation away from steady colors', async () => {
  const { reminderPulse } = await import('../src/lib/reminder.ts')
  assert.equal(reminderPulse('none'), 'none')
  assert.equal(reminderPulse('warn'), 'gentle')
  assert.equal(reminderPulse('due'), 'strong')
  assert.equal(reminderPulse('overdue'), 'none')
})
```

运行 `pnpm test`，确认因 `src/lib/reminder.ts` 不存在而失败。

- [ ] **Step 2: 扩展 API 类型与调用**

在 `api.ts` 增加 camelCase 的 `ReminderConfig`、`Reminder`、`ReminderCheck` 类型，以及：

```ts
export const checkReminders = (expectedTaskId: string) =>
  invoke<ReminderCheck>('check_reminders', { expectedTaskId })
export const setReminderMuted = (expectedTaskId: string, muted: boolean) =>
  invoke<Task>('set_reminder_muted', { expectedTaskId, muted })
export const sendReminderNotification = (title: string, body: string) =>
  invoke<void>('send_reminder_notification', { title, body })
```

`Task` 接口增加 `reminder_muted: boolean`。

- [ ] **Step 3: 实现前端提醒工具**

`src/lib/reminder.ts`：
- 用 `estSeconds > 0` 防除零；
- 根据当前净耗时和配置返回持续视觉等级；
- `reminderPulse` 仅表示收到新事件后的 3 秒动画强度；
- `playReminderChime` 使用一个懒创建的 `AudioContext`，`soft` 用约 660Hz、`urgent` 用两段约 880Hz 短音；自动播放失败时静默忽略。

- [ ] **Step 4: 集成到小组件**

`WidgetView.svelte`：
- 保存 `reminderConfig` 与 `pulse: { level, until }`；
- 每秒刷新时，如果当前任务存在则调用 `checkReminders(task.id)`；
- 新 reminder 去重按 `task_id + node` 记录已处理集合；
- 对 `sound=true` 播放音效；对 `notify=true` 调用 `sendReminderNotification`；
- 将配置、pulse 与 `onMuteReminder` 传给 `WidgetRunning`；
- 忽略任务切换竞态导致的 `task changed, please refresh`。

`WidgetRunning.svelte`：
- 根据 `reminderLevel` 给主时间加 `warn/near/due/overdue` 颜色类；
- 给 widget 加 3 秒 `pulse-gentle` 或 `pulse-strong`；
- due/overdue 保持红框；超时显示现有 `+mm:ss`；
- 超时时显示 `[静音]` 或 `[已静音]`；点击后调用 `setReminderMuted(task.id, true)` 并更新本地任务。

- [ ] **Step 5: 验证并提交**

Run: `pnpm test && pnpm check && pnpm build`
Expected: 全部通过。

```bash
git add src/lib/api.ts src/lib/reminder.ts tests/reminder.test.ts src/components/WidgetView.svelte src/components/WidgetRunning.svelte
git commit -m "feat(ui): progressive reminder visuals and notifications"
```

---

### Task 3: 设置页与 M3 验收

**Files:**
- Create: `src/lib/settings.ts`
- Create: `src/components/SettingsView.svelte`
- Create: `tests/settings.test.ts`
- Modify: `src/App.svelte`
- Modify: `src/components/TaskForm.svelte`
- Modify: `src/components/WidgetRunning.svelte`
- Modify: `src-tauri/src/commands.rs`
- Create: `docs/m3-acceptance.md`
- Test: `tests/settings.test.ts`

**Interfaces:**
- Consumes Task 1/2 的设置 KV 与 IPC。
- Produces:
  - `SETTINGS` 描述数组：key、label、kind、默认值、范围；
  - `normalizeReminderSettings(values): { values, errors }`
  - IPC command `open_settings`。

- [ ] **Step 1: 先写失败测试**

`tests/settings.test.ts` 至少覆盖：

```ts
test('settings defaults and normalization preserve stage order', async () => {
  const { SETTINGS, normalizeReminderSettings } = await import('../src/lib/settings.ts')
  assert.equal(SETTINGS.find(x => x.key === 'reminder_warn_percent')?.default, '75')
  const result = normalizeReminderSettings({
    reminder_enabled: 'true',
    reminder_warn_percent: '90',
    reminder_near_percent: '75',
    reminder_due_percent: '120',
    reminder_overdue_interval_minutes: '10',
  })
  assert.deepEqual(result.values.reminder_near_percent, '90')
  assert.deepEqual(result.values.reminder_warn_percent, '75')
  assert.deepEqual(result.errors, [])
})

test('invalid numeric values surface errors without saving silently', async () => {
  const { normalizeReminderSettings } = await import('../src/lib/settings.ts')
  const result = normalizeReminderSettings({ reminder_warn_percent: 'abc' })
  assert.equal(result.errors.length, 1)
  assert.match(result.errors[0], /提前预警/)
})
```

运行 `pnpm test`，确认失败。

- [ ] **Step 2: 实现设置模型**

`settings.ts` 定義并使用这些 keys：
- `reminder_enabled`，默认 true；
- `reminder_warn_enabled`, `reminder_warn_percent`, `reminder_warn_sound`；
- `reminder_near_enabled`, `reminder_near_percent`, `reminder_near_sound`；
- `reminder_due_enabled`, `reminder_due_percent`, `reminder_due_notify`；
- `reminder_overdue_enabled`, `reminder_overdue_notify`, `reminder_overdue_interval_minutes`。

百分比范围 `1..=1000`，周期范围 `1..=1440` 分钟；归一化后保证三档递增。

- [ ] **Step 3: 实现设置窗口**

- `commands.rs` 增加 async `open_settings`：已存在则 show/focus，否则创建 hash 为 `index.html#settings`、标题 `traceTime 设置`、逻辑尺寸 `360x480` 的窗口；
- `main.rs` 注册 `open_settings`；
- `App.svelte` 增加 `location.hash === '#settings'` 分支；
- `TaskForm` 头部添加设置按钮；
- `WidgetRunning` 当前任务头部添加紧凑设置按钮；
- 设置页按组渲染总开关、三档视觉节点、超时通知与周期；保存前调用 normalize，保存成功显示“已保存”，失败不写入任何变更。

- [ ] **Step 4: 补充验收清单**

创建 `docs/m3-acceptance.md`，逐条列出人工验收：
1. 新任务在 75%/90%/100% 分别出现琥珀/橙/红状态，颜色保持；
2. near/due 收到新事件时出现短时动画并在约 3 秒回落；
3. due 默认系统通知；warn/near 不系统通知；
4. 超时每 10 分钟重复通知，点击静音后通知停止但红色视觉保留；
5. 设置修改立即影响后续判定，已有已触发节点不重复弹；
6. 设置窗口保存非法值时不写入；
7. `pnpm test`、`pnpm check`、`pnpm build`、`cd src-tauri && cargo test` 全绿。

- [ ] **Step 5: 验证并提交**

Run: `pnpm test && pnpm check && pnpm build && cd src-tauri && cargo test`
Expected: 全部通过。

```bash
git add src/lib/settings.ts src/components/SettingsView.svelte tests/settings.test.ts src/App.svelte src/components/TaskForm.svelte src/components/WidgetRunning.svelte src-tauri/src/commands.rs src-tauri/src/main.rs docs/m3-acceptance.md
git commit -m "feat(settings): reminder preferences window"
```
