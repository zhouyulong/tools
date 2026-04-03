# dida365 Task Sync Skill

将 dida365 工作列表中的任务同步到本地 Markdown 文档。
WARNING: 本技能目前仅适用于Windows电脑工作日同步工作任务，请勿用于其他用途。

## 功能特性

- ✅ 从 dida365 获取今日工作列表任务
- ✅ 同步任务到指定 Markdown 文档
- ✅ 智能匹配日期标题，自动创建或更新
- ✅ 保留任务完成状态
- ✅ 支持子任务同步
- ✅ 保留本地手动添加的任务

## 安装

1. 将本 skill 复制到  skills 目录：
   ```
   ~/.workbuddy/skills/dida365-sync/
   ```

2. 确保 Dida365 CLI skill 已安装并可正常使用

## 使用方法

### 通过 WorkBuddy 使用

在 WorkBuddy 中提出需求：

```
"帮我把 dida365 今天工作列表的任务同步到我的待办文档"
```

### 命令行使用（开发测试）

```bash
# 同步今日任务
cd ~/.workbuddy/skills/dida365-sync/scripts
python dida365_sync.py /path/to/tasks.md 2026-03-26
```

## Markdown 文档格式

本技能期望的 Markdown 格式：

```markdown
# 2026年3月26日（周四）
- [x] 已完成任务
  - [x] 已完成子任务
  - [ ] 未完成子任务
- [ ] 未完成任务
```

### 格式说明

- 日期标题：`# YYYY年M月D日（周几）`
- 已完成任务：`- [x] 任务名`
- 未完成任务：`- [ ] 任务名`
- 子任务：缩进 2 空格，相同格式

## 工作流程

1. 从 dida365 获取今日工作列表任务
2. 读取本地 Markdown 文档
3. 查找或创建今日的日期标题
4. 更新或添加任务，保留手动添加的任务
5. 写入更新后的文档

## 依赖

- Dida365 CLI skill（用于获取任务数据）
- Python 3.x（用于执行同步脚本）

## 注意事项

- 同步不会删除本地手动添加的任务
- 任务完成状态以 dida365 为准
- 建议定期同步，保持数据一致

## 开发说明

### 主要文件

- `SKILL.md` - 技能主文件，包含使用指南
- `scripts/dida365_sync.py` - 同步脚本实现
- `references/dida365_cli.md` - Dida365 CLI 集成参考
- `assets/example_todo.md` - 示例 Markdown 格式

### 扩展功能

如需扩展功能，可以修改 `dida365_sync.py`：

- 支持自定义日期格式
- 支持多个工作列表同步
- 添加任务优先级标识
- 支持任务标签同步

---

*本 skill 为 OpenClaw 项目定制开发*
