# tools

个人效率工具集合，包含 Claude Code 辅助技能和日常开发小工具。

## 项目结构

```
tools/
├── claude-model-switch/      # Claude Code 模型快速切换工具
├── codex-switch-script/      # Codex 多供应商/模型一键切换工具
├── skills/
│   ├── get-news/             # 晨间新闻聚合 Skill
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

### codex-switch-script
Codex 多供应商/模型一键切换脚本（PowerShell），通过改写 `~/.codex/config.toml` 和同步环境变量实现。支持供应商的增删查改、会话搜索与供应商改写（含桌面端 SQLite 注册表同步）等功能。内置 UTF-8 编码兼容，避免中文输出乱码。

```powershell
.\switch-codex-provider.ps1 -List                          # 列出所有已配置供应商
.\switch-codex-provider.ps1 -Provider sharedchat           # 切换到指定供应商（使用默认模型）
.\switch-codex-provider.ps1 -P sharedchat -M gpt-5.6-sol   # 切换供应商并指定模型
.\switch-codex-provider.ps1 -Show                          # 查看当前生效配置
.\switch-codex-provider.ps1 -Add myrelay -BaseUrl "https://xxx.com/v1" -ApiKey "sk-xxx" -Model "gpt-5.6-sol"  # 添加供应商
.\switch-codex-provider.ps1 -Remove myrelay                # 删除供应商
.\switch-codex-provider.ps1 -Find "关键词"                  # 按关键词搜索会话（首条消息/标题/工作目录）
.\switch-codex-provider.ps1 -Session <session_id>          # 改写指定会话的供应商
```

供应商清单存储在脚本同目录的 `providers.json` 中，可直接手动编辑。

> **新功能**：`-Find` 通过查询桌面端 SQLite 会话注册表（`state_5.sqlite`）搜索历史会话，配合 `-Session` 可快速定位并改写旧会话的供应商绑定。`-Session` 改写时会自动同步桌面端注册表。

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
