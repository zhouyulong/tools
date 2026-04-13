# tools

个人效率工具集合，包含 Claude Code 辅助技能和日常开发小工具。

## 项目结构

```
tools/
├── claude-model-switch/    # Claude Code 模型快速切换工具
├── skills/
│   ├── get-news/           # 晨间新闻聚合 Skill
│   └── dida365-working-sync/ # 滴答清单 working 项目同步 Skill
└── README.md
```

## 工具说明

### claude-model-switch
快速切换 Claude Code 使用的 AI 模型，通过替换 `~/.claude/settings.json` 配置文件实现。

```bash
claudemodel <model-name>    # 切换模型
claudemodel current         # 查看当前模型
```

### get-news (Claude Skill)
每日自动聚合高价值新闻：世界局势、中东专题、经济政策、AI 技术动态、学术成果等，并做多源交叉验证。

```bash
python scripts/morning_news.py --today
```

### dida365-working-sync (Claude Skill)
将滴答清单 `working` 项目的每日任务同步到本地 Markdown 文档，自动区分待完成和已完成任务。

```bash
python scripts/dida365_working_sync.py
```

## License

MIT
