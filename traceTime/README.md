# traceTime

桌面悬浮计时小组件：记录完成任务的真实时间与轨迹。

## 启动（开发模式）

    pnpm install
    pnpm tauri dev

## 当前功能（M1）

- 创建任务（名称 + 预计时长 + 标签）并立即开始计时
- 暂停（记录原因：临时离开 / 被干扰 / 切换任务 / 其他）与继续
- 完成 / 放弃，历史任务列表查询（命令行 IPC 或后续 UI）
- 数据本地存储于系统应用数据目录
- 置顶悬浮窗：不进任务栏，配置级不抢焦点

## 技术栈

Tauri 2 + Svelte 5 + SQLite (rusqlite)。设计文档见 `docs/2026-11-26-tracetime-design.md`。
