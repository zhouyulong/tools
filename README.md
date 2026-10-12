# tools

个人效率工具集合，包含 Claude Code 辅助技能和日常开发小工具。

## 项目结构

```
tools/
├── claude-model-switch/      # Claude Code 模型快速切换工具
├── codex-switch-script/      # Codex 多供应商/模型一键切换工具
├── skills/
│   ├── ccf-prioritized-literature-search-package/  # CCF 顶会/顶刊优先文献检索 Skill（可移植打包）
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
Codex 多供应商/模型一键切换脚本（PowerShell），通过改写 `~/.codex/config.toml` 和同步环境变量实现。支持官方/自定义供应商切换、模型自动探测与交互选择、供应商的增删查改、会话搜索与供应商改写（含桌面端 SQLite 注册表同步）等功能。内置 UTF-8 编码兼容，避免中文输出乱码。

```powershell
.\switch-codex-provider.ps1 -List                          # 列出所有已配置供应商（含内置官方 official）
.\switch-codex-provider.ps1 -Provider sharedchat           # 切换：自动探测可用模型，交互选择（选中即回写默认模型）
.\switch-codex-provider.ps1 -P sharedchat -M gpt-5.6-sol   # 显式指定模型（跳过菜单，仍静默探测用于 catalog；不回写默认模型）
.\switch-codex-provider.ps1 -P sharedchat -Auto            # 跳过菜单，直接用该供应商默认模型（仍静默探测用于 catalog）
.\switch-codex-provider.ps1 -Probe sharedchat              # 只探测列出可用模型，不切换
.\switch-codex-provider.ps1 -P official                    # 切回 OpenAI 官方（ChatGPT 登录走 auth.json，自动清理劫持环境变量）
.\switch-codex-provider.ps1 -Show                          # 查看当前生效配置（模式判定+环境变量残留警告）
.\switch-codex-provider.ps1 -Add myrelay -BaseUrl "https://xxx.com/v1" -ApiKey "sk-xxx" -Model "gpt-5.6-sol"  # 添加供应商
.\switch-codex-provider.ps1 -Remove myrelay                # 删除供应商（official 不可删）
.\switch-codex-provider.ps1 -Find "关键词"                  # 按关键词搜索会话（首条消息/标题/工作目录）
.\switch-codex-provider.ps1 -Session <session_id>          # 改写指定会话的供应商
```

供应商清单存储在脚本同目录的 `providers.json` 中，可直接手动编辑。

> **官方供应商**：`official` 为内置条目（清单缺失时自动补种）。base_url/api_key 留空 = ChatGPT 订阅登录（auth.json）；给 official 填 api_key 则走 OpenAI API Key 模式。切回官方时会删除 config.toml 根键 `model_provider` 与 `model_catalog_json`（恢复内置 openai provider 与内置模型目录），订阅登录模式下还会自动删除 DeepSeek/中转安装脚本遗留的 `preferred_auth_method="apikey"` / `forced_login_method="api"`（这两个键会强制只能 API Key 登录、禁用账号登录；官方配 api_key 时保留），并清理会劫持官方请求的 `OPENAI_BASE_URL` 与残留 Key 环境变量。
>
> **模型探测**：relay 走 `{base_url}/models`（优先 curl.exe——PS5.1/.NET 的 TLS 指纹会被 Cloudflare 类防护挑战 403，curl 指纹可过；Invoke-RestMethod 兜底）；官方读桌面端缓存 `~/.codex/models_cache.json`（缓存过期时仅作参考）。探测失败（如部分中转站不支持列表接口）自动降级为手动输入模型 ID。
>
> **模型 catalog（桌面端模型显示的真相源）**：ChatGPT 桌面端的模型列表/显示名由 config.toml 根级 `model_catalog_json` 决定（全局键，不区分供应商——这正是切换供应商后桌面端显示错模型的原因）。relay 切换时脚本自动生成专属 catalog `~/.codex/model-providers/catalog-<id>.json` 并重指根键，内容合并四路来源：本次探测结果 + 旧 catalog 携带条目 + `~/.codex/models.json`（保留 DeepSeek 等既有目录的 `model_messages` 模板）+ 官方缓存条目；探测不到的模型按官方模板克隆补位。切回官方时移除该键还原内置目录。`-Show` 会输出 catalog 诊断（存在性/模型数/当前模型是否在册）。
>
> **会话改写**：`-Find` 通过查询桌面端 SQLite 会话注册表（`state_5.sqlite`）搜索历史会话，配合 `-Session` 可快速定位并改写旧会话的供应商绑定；`-Session -P official` 会自动映射为内置 id `openai`。改写时自动同步桌面端注册表。
>
> **部署副本**：实际运行版本在 `~/.codex/model-providers/`（其 providers.json 含真实 key）；本目录的 providers.json 为脱敏模板。更新脚本后需同步拷贝到部署目录。注意：ChatGPT 桌面端运行中会自行回写 config.toml 的 `model`/`model_reasoning_effort` 键，切换供应商/模型后需托盘退出重开桌面端。

### ccf-prioritized-literature-search (Claude Skill)
面向计算机领域的「顶会/顶刊优先」文献检索 Skill。当用户的检索诉求同时包含质量与时效（"顶刊/顶会/顶级/高质量/优质/前沿/最新"等）时触发，默认时间窗为检索日前 12 个月（用户给定区间可覆盖），按以下优先级取样并逐条给出场地证据标签：

1. CCF A 期刊与会议
2. CCF B 会议（仅当 CCF A 结果不足；AI / HCI 与泛在计算 / 交叉综合与新兴方向须并入专用补充索引中的全部相关会议）
3. 经 EasyScholar 校验为 SCI一区的 CCF B 期刊
4. 其他经 EasyScholar 校验为 SCI一区的期刊

内置数据：2026 年（第七版）CCF A/B 会议与期刊目录、上述三个方向的 CCF B 会议补充索引（28 个）、以及经 EasyScholar 校验的 CCF B SCI一区缓存（2026-10-11 校验，90 天内有效）。实时校验仅在环境变量中读取 `EASY_SCHOLAR_KEY`，绝不打印、落盘或写入 URL；缺失时引导用户自行在 EasyScholar 开放 API 控制台获取。无明确证据一律判为"未验证"，不得臆断 SCI一区或"顶级"。

```bash
python ccf-prioritized-literature-search/scripts/easyscholar_rank.py                            # 查询期刊分区
python ccf-prioritized-literature-search/scripts/refresh_ccf_b_sci_q1_cache.py --confirm-batch  # 刷新 CCF B SCI一区缓存
```

> 该 Skill 以「目录型可移植包」形式提供（`skills/ccf-prioritized-literature-search-package/`）：`manifest.json` 为包清单，`TRANSFER.md` 为迁移说明——把内层 `ccf-prioritized-literature-search/` 目录整体拷入目标环境的 skill 目录即可，`agents/openai.yaml` 为 Codex 可选的 UI 元数据，其他环境可忽略。脚本仅依赖 Python 3.9+ 标准库，包内不含任何密钥。

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
