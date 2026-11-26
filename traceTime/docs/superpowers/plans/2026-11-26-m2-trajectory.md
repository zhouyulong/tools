# traceTime M2（轨迹完整）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付轨迹完整能力：打断切换（自动创建关联子任务）、长暂停质询、历史列表窗口与任务详情时间轴。

**Architecture:** 延续 M1：Rust 层为唯一事实来源，前端展示与发起动作。单任务约束修正为"同一时刻至多一个 running 任务，暂停任务可共存"；打断切换时新任务携带 `parent_task_id`，父任务保持暂停。

**Tech Stack:** 同 M1（Tauri 2、Svelte 5、rusqlite）。

**Spec:** `docs/2026-11-26-tracetime-design.md`（第 3.1/3.3 节、数据模型、状态机）

## Global Constraints

- 打断切换语义：选"切换任务"后父任务暂停（reason=switch），新任务自动开始计时并携带 `parent_task_id`；父任务保持暂停，子任务结束后由用户手动继续
- 并发约束修正：create_task 仅在**存在 running 任务**时拒绝（"a task is already running"）；paused 任务不阻塞创建，但通过表单直接创建的新任务不带 parent_task_id，只有切换流程携带
- current 语义：优先返回 running 任务；无 running 时返回最近创建的非终态 paused 任务
- 长暂停阈值：settings 表 KV `long_pause_hours`，默认 `2`；本次只做读取（设置 UI 属 M3）
- 时间轴视图配色：绿=work、灰=pause、红=anomaly（M1 无 anomaly 数据，视图预留）
- 所有新查询/写入延续 M1 原则：段与事件只追加；写路径走事务；仅 `QueryReturnedNoRows` 表示不存在
- Windows/Linux/macOS 三平台行为以 M1 既有方式为准，本计划不引入新平台原生代码
- 每个 Rust 任务完成后 `cargo test` 全绿并提交一次

---

### Task 1: 后端——并发约束修正 + 打断关联 + 暂停时长暴露

**Files:**
- Modify: `src-tauri/src/tracker.rs`
- Test: `src-tauri/src/tracker.rs` 内 tests 模块追加

**Interfaces:**
- Consumes: M1 Tracker（create_task/pause/resume/finish/abandon/current）
- Produces:
  - `create_task(&self, name, est_seconds, tag, parent_task_id: Option<&str>) -> Result<TaskDto, String>`：存在 running 任务时 `Err("a task is already running")`；`parent_task_id` 必须指向存在的非终态任务，否则 `Err("parent task not found or terminal")`
  - `TaskDto` 新增字段 `paused_since: Option<String>`（status=paused 时为最近 paused 事件 at，否则 None）
  - `pub fn list_paused(&self) -> Result<Vec<TaskDto>, String>`（按 created_at 倒序，全部 paused）
  - current/list_finished 行为不变（current 增加上述优先级语义）

- [ ] **Step 1: 追加失败测试**

```rust
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
    let child = tr.create_task("紧急修复", 900, Some("行政"), Some(&parent.id)).unwrap();
    assert_eq!(child.status, "running");
    let conn = tr.conn.lock().unwrap();
    let p: String = conn
        .query_row("SELECT parent_task_id FROM tasks WHERE id=?1", [&child.id], |r| r.get(0))
        .unwrap();
    assert_eq!(p, parent.id);
    drop(conn);
    // current 显示子任务；子任务结束后回到父任务（paused，含 paused_since）
    assert_eq!(tr.current().unwrap().unwrap().id, child.id);
    tr.finish().unwrap();
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
    tr.finish().unwrap();
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
```

Run: `cd src-tauri && cargo test`
Expected: 编译失败（create_task 参数不匹配）。

- [ ] **Step 2: 实现语义修改**

要点（在现有 tracker.rs 上修改）：

```rust
// TaskDto 增加
pub paused_since: Option<String>,

fn running_task(conn: &Connection) -> Option<TaskDto> {
    let id: String = conn
        .query_row("SELECT id FROM tasks WHERE status='running' LIMIT 1", [], |r| r.get(0))
        .ok()?;
    Self::get_task(conn, &id)
}

fn latest_paused(conn: &Connection) -> Option<TaskDto> {
    let id: String = conn
        .query_row(
            "SELECT id FROM tasks WHERE status='paused' ORDER BY created_at DESC LIMIT 1",
            [], |r| r.get(0),
        )
        .ok()?;
    Self::get_task(conn, &id)
}

fn paused_since(conn: &Connection, task_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT at FROM events WHERE task_id=?1 AND kind='paused' ORDER BY id DESC LIMIT 1",
        [task_id], |r| r.get(0),
    ).ok()
}

// with_live_net 同时补 paused_since（仅 paused 任务）
// active_task 改为 running_task().or(latest_paused())
// create_task 签名加 parent_task_id:
//   - running_task 存在 => Err("a task is already running")
//   - parent_task_id 有的 => 校验父任务存在且 status IN ('paused','running')，否则 Err("parent task not found or terminal")
//   - INSERT tasks 时写入 parent_task_id
// list_paused: SELECT * FROM tasks WHERE status='paused' ORDER BY created_at DESC
// 既有测试中 create_task 调用全部补 None 参数；"another task is active" 错误文案更新为 "a task is already running"（同步改对应测试断言）
```

- [ ] **Step 3: 全量测试通过**

Run: `cd src-tauri && cargo test`
Expected: 原 20 个测试（含改断言）+ 新 4 个全部 PASS。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tracker.rs
git commit -m "feat(tracker): interruption linking, multi-paused tasks, paused_since"
```

---

### Task 2: 后端——设置 KV、任务详情与暂停列表查询

**Files:**
- Modify: `src-tauri/src/tracker.rs`
- Test: tracker.rs tests 追加

**Interfaces:**
- Produces:
  - `pub fn get_setting(&self, key: &str) -> Result<Option<String>, String>`
  - `pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String>`
  - `pub fn setting_or(&self, key: &str, default: &str) -> Result<String, String>`
  - `pub struct SegmentDto { started_at: String, ended_at: String, kind: String }`（serde Serialize）
  - `pub struct EventDto { at: String, kind: String, detail: Option<String> }`（serde Serialize）
  - `pub struct TaskDetailDto { task: TaskDto, segments: Vec<SegmentDto>, events: Vec<EventDto> }`（serde Serialize）
  - `pub fn task_detail(&self, task_id: &str) -> Result<TaskDetailDto, String>`（任务不存在时 Err("task not found")）

- [ ] **Step 1: 追加失败测试**

```rust
#[test]
fn settings_roundtrip_and_default() {
    let tr = t();
    assert_eq!(tr.get_setting("long_pause_hours").unwrap(), None);
    assert_eq!(tr.setting_or("long_pause_hours", "2").unwrap(), "2");
    tr.set_setting("long_pause_hours", "1").unwrap();
    assert_eq!(tr.setting_or("long_pause_hours", "2").unwrap(), "1");
}

#[test]
fn task_detail_returns_segments_and_events_in_order() {
    let tr = t();
    let task = tr.create_task("A", 60, None, None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    tr.pause("rest").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    tr.resume().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    tr.finish().unwrap();
    let d = tr.task_detail(&task.id).unwrap();
    assert_eq!(d.task.id, task.id);
    assert_eq!(d.segments.len(), 2);
    assert!(d.segments.iter().all(|s| s.kind == "work"));
    let kinds: Vec<&str> = d.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["created", "started", "paused", "resumed", "finished"]);
    let err = tr.task_detail("nope").unwrap_err();
    assert_eq!(err, "task not found");
}
```

- [ ] **Step 2: 实现**

```rust
#[derive(Debug, Serialize, Clone)]
pub struct SegmentDto { pub started_at: String, pub ended_at: String, pub kind: String }
#[derive(Debug, Serialize, Clone)]
pub struct EventDto { pub at: String, pub kind: String, pub detail: Option<String> }
#[derive(Debug, Serialize, Clone)]
pub struct TaskDetailDto {
    pub task: TaskDto, pub segments: Vec<SegmentDto>, pub events: Vec<EventDto>,
}

pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
    let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
    match conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
    let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [key, value],
    ).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn setting_or(&self, key: &str, default: &str) -> Result<String, String> {
    Ok(self.get_setting(key)?.unwrap_or_else(|| default.to_string()))
}
pub fn task_detail(&self, task_id: &str) -> Result<TaskDetailDto, String> {
    let conn = self.conn.lock().map_err(|_| "lock poisoned")?;
    let task = match Self::get_task(&conn, task_id) {
        Some(t) => Self::with_live_net(&conn, t),
        None => return Err("task not found".into()),
    };
    let mut stmt = conn.prepare(
        "SELECT started_at, ended_at, kind FROM segments WHERE task_id=?1 ORDER BY started_at, id",
    ).map_err(|e| e.to_string())?;
    let segments = stmt.query_map([task_id], |r| SegmentDto {
        started_at: r.get(0)?, ended_at: r.get(1)?, kind: r.get(2)?,
    }).map_err(|e| e.to_string())?
      .filter_map(|r| r.ok()).collect();
    let mut stmt = conn.prepare(
        "SELECT at, kind, detail FROM events WHERE task_id=?1 ORDER BY at, id",
    ).map_err(|e| e.to_string())?;
    let events = stmt.query_map([task_id], |r| EventDto {
        at: r.get(0)?, kind: r.get(1)?, detail: r.get(2)?,
    }).map_err(|e| e.to_string())?
      .filter_map(|r| r.ok()).collect();
    Ok(TaskDetailDto { task, segments, events })
}
```

Run: `cd src-tauri && cargo test`
Expected: 全部 PASS（含既有 24 个）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tracker.rs
git commit -m "feat(tracker): settings kv and task detail queries"
```

---

### Task 3: IPC 命令扩展 + 前端 api.ts

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`, `src/lib/api.ts`
- Test: `cargo build` + `pnpm check`

**Interfaces:**
- Produces（前端 api.ts 新增导出）:
  - `createTask(name, estSeconds, tag?, parentTaskId?)`
  - `listPaused(): Promise<Task[]>`
  - `taskDetail(id): Promise<TaskDetail>`；`TaskDetail = { task: Task, segments: Segment[], events: Event[] }`
  - `getSetting(key): Promise<string | null>`、`setSetting(key, value): Promise<void>`
  - `Task` 类型增加 `paused_since: string | null`

- [ ] **Step 1: 命令层**

commands.rs 新增/修改（模式与 M1 一致，全部 `State<Tracker>` + Result<_, String>）：

```rust
#[tauri::command]
pub fn create_task(
    state: State<'_, Tracker>,
    name: String,
    est_seconds: i64,
    tag: Option<String>,
    parent_task_id: Option<String>,
) -> R<TaskDto> {
    state.create_task(&name, est_seconds, tag.as_deref(), parent_task_id.as_deref())
}

#[tauri::command]
pub fn list_paused(state: State<'_, Tracker>) -> R<Vec<TaskDto>> { state.list_paused() }

#[tauri::command]
pub fn task_detail(state: State<'_, Tracker>, task_id: String) -> R<TaskDetailDto> {
    state.task_detail(&task_id)
}

#[tauri::command]
pub fn get_setting(state: State<'_, Tracker>, key: String) -> R<Option<String>> {
    state.get_setting(&key)
}

#[tauri::command]
pub fn set_setting(state: State<'_, Tracker>, key: String, value: String) -> R<()> {
    state.set_setting(&key, &value)
}
```

main.rs 的 generate_handler 追加这 5 个命令。

- [ ] **Step 2: api.ts 更新**

```ts
export interface Task {
  // 既有字段...
  paused_since: string | null
}
export interface Segment { started_at: string; ended_at: string; kind: string }
export interface Event { at: string; kind: string; detail: string | null }
export interface TaskDetail { task: Task; segments: Segment[]; events: Event[] }

export const createTask = (name: string, estSeconds: number, tag?: string, parentTaskId?: string) =>
  invoke<Task>('create_task', {
    name, estSeconds: Math.round(estSeconds), tag: tag ?? null, parentTaskId: parentTaskId ?? null,
  })
export const listPaused = () => invoke<Task[]>('list_paused')
export const taskDetail = (id: string) => invoke<TaskDetail>('task_detail', { taskId: id })
export const getSetting = (key: string) => invoke<string | null>('get_setting', { key })
export const setSetting = (key: string, value: string) => invoke<void>('set_setting', { key, value })
```

Run: `cd src-tauri && cargo build && cd .. && pnpm check`
Expected: 编译通过；svelte-check 0 错误（TaskForm 对 createTask 的调用可不带第 4 参）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/main.rs src/lib/api.ts
git commit -m "feat(ipc): interruption, settings and detail commands"
```

---

### Task 4: 小组件 UI——切换任务流程 + 回到暂停任务 + 长暂停质询

**Files:**
- Create: `src/components/SwitchTaskForm.svelte`, `src/components/LongPauseBanner.svelte`
- Modify: `src/components/WidgetRunning.svelte`, `src/components/TaskForm.svelte`, `src/App.svelte`
- Test: `pnpm check` + `pnpm build`；GUI 项进验收清单

**Interfaces:**
- Consumes: Task 3 的 api.ts
- Produces:
  - SwitchTaskForm：内嵌于 WidgetRunning 的迷你表单（名称/预计分钟/标签），提交调 `createTask(name, min*60, tag, task.id)`，成功后关闭表单（轮询自然切到子任务）；带"取消"回到原因菜单
  - TaskForm：顶部列出 paused 任务 chips（`listPaused()`，显示名称 + 已暂停时长），点 chip 调 `resume()` 不行——多暂停任务 resume 的是 latest；改为 chip 点击显示该任务为主当前（不做，M2 简化：chip 只显示信息 + "继续最近暂停的任务"按钮调 resume()）；精确选择留 M3——此裁决写入计划
  - LongPauseBanner：task.status==='paused' 且 `now - paused_since > long_pause_hours*3600` 时显示"任务 X 已暂停 N 小时"+ 继续/完成/放弃三按钮（放弃二次确认）
  - App.svelte：轮询循环中并行刷新 paused 列表；把 long_pause_hours 通过 getSetting 读出（默认 2）传给 LongPauseBanner

- [ ] **Step 1: SwitchTaskForm.svelte**

```svelte
<script lang="ts">
  import { createTask, TAG_PRESETS } from '../lib/api'

  export let parentTaskId: string
  let name = ''
  let estMin = 25
  let tag = localStorage.getItem('traceTime:lastTag') ?? '科研'
  let busy = false
  let error = ''

  async function submit() {
    if (busy || !name.trim()) return
    busy = true; error = ''
    try {
      await createTask(name, estMin * 60, tag === '__none__' ? undefined : tag, parentTaskId)
      if (tag !== '__none__') localStorage.setItem('traceTime:lastTag', tag)
    } catch (e) { error = String(e) }
    busy = false
  }
</script>

<div class="switch-form">
  <input placeholder="新任务名" bind:value={name} />
  <div class="row">
    <input type="number" min="1" bind:value={estMin} title="预计分钟" />
    <select bind:value={tag}>
      {#each TAG_PRESETS as t}<option value={t}>{t}</option>{/each}
      <option value="__none__">无标签</option>
    </select>
  </div>
  <div class="row">
    <button on:click={submit} disabled={busy || !name.trim()}>开始新任务</button>
    <button class="ghost" on:click={() => dispatch('cancel')}>取消</button>
  </div>
  {#if error}<p class="error">{error}</p>{/if}
</div>
<script context="module" lang="ts">
  import { createEventDispatcher } from 'svelte'
</script>
```

注意：Svelte 5 中 dispatcher 需在实例脚本声明——把 `const dispatch = createEventDispatcher()` 放主 script 顶部，`dispatch('cancel')` 供父组件关闭表单；上面骨架需按此整理。父组件（WidgetRunning）新增状态 `mode: 'normal' | 'pause-menu' | 'switch-form'`：选"切换任务"进入 switch-form，SwitchTaskForm 成功或取消后回到 normal。

- [ ] **Step 2: TaskForm 暂停任务 chips**

TaskForm onMount 调 `listPaused()`；有暂停任务时渲染：

```svelte
{#if paused.length}
  <div class="paused-list">
    {#each paused as t}
      <span class="chip">{t.name} · 已暂停 {fmtHMS(pausedFor(t))}</span>
    {/each}
    <button on:click={() => resume().then(refreshAll)}>继续最近暂停的任务</button>
  </div>
{/if}
```

`pausedFor(t) = (Date.now() - new Date(t.paused_since!).getTime()) / 1000`。裁决记录：精确选择某个暂停任务继续属于 M3（多任务管理 UI），M2 只提供"继续最近"。

- [ ] **Step 3: LongPauseBanner + App 接线**

```svelte
<script lang="ts">
  import { resume, finishTask, abandonTask } from '../lib/api'
  import { fmtHMS } from '../lib/format'
  export let task: Task
  export let thresholdHours: number
  export let nowTick: number // 父组件每秒刷新的 tick，保证重算
  let confirmAbandon = false
  $: pausedFor = task.paused_since
    ? (nowTick - new Date(task.paused_since).getTime()) / 1000 : 0
  $: visible = task.status === 'paused' && pausedFor > thresholdHours * 3600
</script>

{#if visible}
  <div class="banner">
    已暂停 {fmtHMS(pausedFor)}——继续 / 完成 / 放弃？
    <button on:click={() => resume()}>继续</button>
    <button on:click={() => finishTask()}>完成</button>
    <button on:click={() => (confirmAbandon ? abandonTask() : (confirmAbandon = true))}>
      {confirmAbandon ? '确认放弃?' : '放弃'}
    </button>
  </div>
{/if}
```

App.svelte：`let nowTick = Date.now()`，轮询里更新；`getSetting('long_pause_hours')` 启动时读一次（失败/为空用 "2"）；WidgetRunning 与 TaskForm 状态下都渲染 LongPauseBanner（paused 任务可能不是当前显示任务——M2 简化：Banner 只挂在当前显示的 paused 任务上；运行中子任务时不提示父任务，由 TaskForm chips 覆盖）。此裁决写入计划。

- [ ] **Step 4: 验证与提交**

Run: `pnpm check && pnpm build`
Expected: 0 错误。GUI 行为进 m2 验收清单。

```bash
git add src/components src/App.svelte
git commit -m "feat(ui): switch-task flow, paused chips and long-pause banner"
```

---

### Task 5: 历史窗口与任务详情时间轴

**Files:**
- Create: `src/components/HistoryView.svelte`, `src/components/TimelineBar.svelte`
- Modify: `src/App.svelte`（hash 路由）, `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`（open_history 命令）, `src-tauri/Cargo.toml`（如需 `tauri` 的 window 特性，默认已有）
- Test: `cargo build` + `pnpm check` + `pnpm build`；GUI 项进验收清单

**Interfaces:**
- Produces:
  - IPC `open_history()`：Rust 侧用 WebviewWindowBuilder 创建/显示 480×520 置顶窗口，URL `index.html#history`
  - App.svelte：`location.hash === '#history'` 时渲染 HistoryView，否则渲染小组件
  - HistoryView：`listFinished()` 按日分组（finished_at 本地时区日期）；左侧列表（名称/净耗时/偏差标记），右侧选中任务 `taskDetail` 的统计（净/墙钟/预计/偏差）+ TimelineBar
  - TimelineBar：接收 segments，按总跨度比例渲染横向条（work 绿、pause 灰、其他红），段间隙按 started_at 差计算

- [ ] **Step 1: Rust open_history 命令**

```rust
use tauri::{WebviewWindowBuilder, WebviewUrl};

#[tauri::command]
pub fn open_history(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("history") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().ok();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "history", WebviewUrl::App("index.html#history".into()))
        .title("traceTime 历史")
        .inner_size(480.0, 520.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

`use tauri::Manager;` 需要（get_webview_window）。注册进 generate_handler。

- [ ] **Step 2: 前端路由与小组件入口按钮**

App.svelte 顶部：

```svelte
<script lang="ts">
  const isHistory = location.hash === '#history'
</script>
{#if isHistory}
  <HistoryView />
{:else}
  <!-- 既有小组件 UI；在 TaskForm 区域加一个"历史"入口 -->
{/if}
```

TaskForm 加 `<button class="ghost" on:click={() => invoke('open_history')}>历史</button>`（api.ts 加 `openHistory()` 包装）。

- [ ] **Step 3: HistoryView + TimelineBar**

HistoryView 骨架（实际代码按此实现，样式与小组件一致）：

```svelte
<script lang="ts">
  import { onMount } from 'svelte'
  import { listFinished, taskDetail, type Task, type TaskDetail } from '../lib/api'
  import TimelineBar from './TimelineBar.svelte'
  import { fmtHMS } from '../lib/format'

  let tasks: Task[] = []
  let selected: TaskDetail | null = null
  let groups: { day: string; items: Task[] }[] = []

  function dayOf(t: Task): string {
    return new Date(t.finished_at!).toLocaleDateString('zh-CN')
  }
  function regroup() {
    const map = new Map<string, Task[]>()
    for (const t of tasks) {
      const d = dayOf(t)
      if (!map.has(d)) map.set(d, [])
      map.get(d)!.push(t)
    }
    groups = [...map.entries()].map(([day, items]) => ({ day, items }))
  }
  async function select(t: Task) { selected = await taskDetail(t.id) }
  onMount(async () => { tasks = await listFinished(); regroup() })
</script>

<div class="history">
  <aside>
    {#each groups as g}
      <p class="day">{g.day}</p>
      {#each g.items as t}
        <button class:on={selected?.task.id === t.id} on:click={() => select(t)}>
          {t.name} · {fmtHMS(t.net_seconds)}
          {#if t.net_seconds > t.est_seconds}<span class="over">超 {fmtHMS(t.net_seconds - t.est_seconds)}</span>{/if}
        </button>
      {/each}
    {:else}
      <p class="empty">暂无历史任务</p>
    {/each}
  </aside>
  <section>
    {#if selected}
      <h3>{selected.task.name}</h3>
      <p>净 {fmtHMS(selected.task.net_seconds)} / 预计 {fmtHMS(selected.task.est_seconds)}
         · 墙钟 {fmtHMS(gross(selected))}</p>
      <TimelineBar segments={selected.segments} />
      <!-- 段列表 -->
      {#each selected.segments as s}
        <p class="seg">{s.started_at} → {s.ended_at} · {s.kind}</p>
      {/each}
    {:else}
      <p class="empty">选择左侧任务查看轨迹</p>
    {/if}
  </section>
</div>
```

`gross(selected)` 用 `(new Date(selected.task.finished_at!) - new Date(selected.task.created_at))/1000` 秒数传入 fmtHMS。abandoned 任务同样展示（状态徽标）。

TimelineBar：props `segments: Segment[]`；总跨度 = max(ended_at) − min(started_at)；每段渲染绝对定位横条，left/width 按占比；颜色 class 由 kind 决定。

- [ ] **Step 4: 验证与提交**

Run: `pnpm check && pnpm build && cd src-tauri && cargo build`
Expected: 全部通过。

```bash
git add src src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat(ui): history window with task timeline"
```

---

### Task 6: M2 收尾——验收清单与回归

**Files:**
- Create: `docs/m2-acceptance.md`
- Modify: `README.md`（功能列表加 M2 项；历史入口说明）

**Interfaces:**
- Produces: 人工验收清单 + 全量回归记录

- [ ] **Step 1: 写 m2-acceptance.md**

清单（状态：待人工验收）：

1. 运行中任务点 ⏸ → 切换任务 → 迷你表单输入新任务名 → 开始：显示新任务计时，旧任务保持暂停
2. 子任务完成后：小组件回到父任务（已暂停态），▶ 可继续，净耗时不包含子任务时间
3. 暂停任一任务后把系统等待超过阈值（测试时可临时把 long_pause_hours 设为 0.001 或改用 setSetting）→ 重开小组件出现长暂停质询横幅；点继续/完成/放弃均生效
4. 完成两个以上任务后，小组件点"历史"打开历史窗口：按日期分组、可选中任务
5. 历史详情显示净/预计/墙钟与时间轴条（绿=工作、灰=暂停），含暂停段的任务能看到灰色段
6. 空历史窗口显示"暂无历史任务"
7. `pnpm tauri build --no-bundle` 通过（发布构建不回归）
8. `cargo test` 全绿记录

- [ ] **Step 2: 全量回归**

Run: `cd src-tauri && cargo test && cd .. && pnpm check && pnpm build && pnpm tauri build --no-bundle`
结果写入 m2-acceptance.md"回归"小节。

- [ ] **Step 3: README 更新**

M1 功能列表上方加"M2 新增"小节：打断切换、长暂停质询、历史窗口与时间轴。

- [ ] **Step 4: Commit**

```bash
git add docs/m2-acceptance.md README.md
git commit -m "docs: m2 acceptance checklist and readme updates"
```

---

## M2 不含（防执行器越界）

- 提醒系统与设置界面（M3）：ReminderEngine、reminder_fired 写入、四档提醒动画、系统通知、设置页
- est_changed 修改预计时长（M3 随设置一起）
- 毛玻璃背景、闲置淡化、原生不抢焦点、异常恢复、超时静音（M4）
- 多暂停任务的精确选择继续（M3 多任务管理）；anomaly 段的实际产生（M4）
