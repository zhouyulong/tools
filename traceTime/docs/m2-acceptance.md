# M2 手动验收清单

> 状态：待人工验收（用户执行）。本任务不运行 `pnpm tauri dev` 或 UI 自动化。

- [ ] 1. 运行中任务点 ⏸ → 选择“切换任务” → 迷你表单输入新任务名 → 点“开始”：显示新任务计时，旧任务保持暂停。
- [ ] 2. 子任务完成后：小组件回到父任务（已暂停态），点 ▶ 可继续，父任务净耗时不包含子任务时间。
- [ ] 3. 暂停任一任务后把系统等待超过阈值（测试时可临时把 `long_pause_hours` 设为 `0.001`，或通过 `setSetting` / 直接调用 `getSetting`、`setSetting` IPC 调整阈值）→ 重开小组件出现长暂停质询横幅；点继续 / 完成 / 放弃均生效。
- [ ] 4. 完成两个以上任务后，小组件点“历史”打开历史窗口：按日期分组，可选中任务。
- [ ] 5. 历史详情显示净时长 / 预计 / 墙钟与时间轴条（绿=工作、灰=暂停）；含暂停段的任务能看到灰色段。
- [ ] 6. 空历史窗口显示“暂无历史任务”。
- [ ] 7. `pnpm tauri build --no-bundle` 通过（发布构建不回归）。
- [ ] 8. `cargo test` 全绿记录。

## 回归

- 执行日期：2026-11-26
- `cd src-tauri && cargo test`：通过，28 passed；0 failed；0 ignored；0 measured；0 filtered out。
- `pnpm check`：通过，`svelte-check` 与 TypeScript 均为 0 errors、0 warnings。
- `pnpm build`：通过，Vite 生产构建成功。
- `pnpm tauri build --no-bundle`：通过，生成 `src-tauri/target/release/app`。构建输出含两项既有警告：bundle identifier `dev.tracetime.app` 以 `.app` 结尾，以及 `Tracker::setting_or` 未使用；两者均未导致命令失败。

## 遗留

- 对照设计文档第 12 节，M2 的事件/工作段记录、`interruption_of` 打断关联、多暂停任务共存、长暂停质询、历史列表及任务详情时间轴均已有对应实现入口；本清单第 1–6 条用于由用户完成 GUI 验收，不能由本次命令回归替代。
- 多暂停任务的“精确选择某一暂停任务继续”属于 M3 多任务管理；当前仅提供继续最近暂停任务的入口。
- GUI 验收仍待用户执行，包括父子切换、长暂停横幅、历史空态和含暂停段的时间轴显示。
