# 晨间新闻聚合 (Morning News)

每日自动聚合高价值新闻信息，适合通勤时快速了解全局。

## 功能特性

- **世界局势**：Reuters、BBC、Al Jazeera、UN News
- **中东专题**：专门关注中东地区动态
- **经济政策**：中美欧主要央行政策（Fed、ECB、财新）
- **AI 技术**：Agent、Memory、RAG 等前沿技术动态
- **学术成果**：arXiv、Semantic Scholar 最新论文
- **工程应用**：GitHub Trending AI 项目

## 真实性校验

- ✅ **多源交叉验证**：重要新闻至少 2 个独立信源确认
- ✅ **来源可信度评分**：官方机构、权威媒体优先
- ✅ **时间戳校验**：标注信息获取时间，排除过时信息

## 快速开始

### 1. 安装依赖

```bash
pip install -r requirements.txt
```

### 2. 手动运行

```bash
# 获取默认时间范围（前一天18:00 - 当前）的新闻
python scripts/morning_news.py

# 只获取今天的新闻
python scripts/morning_news.py --today

# 获取指定日期
python scripts/morning_news.py --date 2026-04-01

# 保存到文件
python scripts/morning_news.py --save
```

### 3. 自动定时执行

已配置每天 7:30 自动执行。

查看定时任务：
```
/cron list
```

## 输出示例

```markdown
# 晨报 （北京时间）2026年4月3日 周五

> 信息时间范围：04月02日 18:00 ~ 04月03日 07:30
> 生成时间：07:30
> 信息统计：共 35 条，[已验证/官方] 20 条

---

## 世界局势

### 中东动态
1. **[已验证]** 标题... (Reuters + Al Jazeera)
   - 摘要：...
   - [查看详情](链接)

## 经济政策

### 美国
1. **[官方发布]** 美联储利率决议...
   - (Fed)

## AI 技术动态

### AI Agent / Memory
1. **[社区讨论]** 新开源 Agent 框架...
   - 🔥 450 points | 💬 89 comments
```

## 可信度标签说明

| 标签 | 含义 |
|------|------|
| [已验证] | 2+ 独立信源确认 |
| [官方发布] | 政府/机构官方发布 |
| [单一信源] | 仅一个来源，建议核实 |
| [社区讨论] | Hacker News/GitHub 热度 |
| [学术] | 来自 arXiv/Semantic Scholar |

## 目录结构

```
morning-news/
├── SKILL.md              # Skill 定义文件
├── README.md             # 本文件
├── requirements.txt      # Python 依赖
├── scripts/
│   └── morning_news.py   # 主脚本
└── reports/              # 生成的报告（自动创建）
    └── news_20260403.md
```

## 自定义配置

编辑 `scripts/morning_news.py` 中的以下配置：

```python
# RSS 源配置
RSS_SOURCES = {
    "world": {...},
    "economy_cn": {...},
    ...
}

# 可信度评分
CREDIBILITY = {...}

# AI 关键词
AI_KEYWORDS = [...]
```

## 注意事项

1. **网络要求**：需要访问国际网络（Reuters、Hacker News 等）
2. **API 限制**：部分源有频率限制，请勿过于频繁调用
3. **信息时效**：新闻具有时效性，建议每日查看
4. **真实性**：虽然有多重校验，但重要决策仍需核实原始来源
5. **时区**：所有时间均使用北京时间

## 与 Claude Code 集成

将本目录放在项目根目录下，Claude Code 会自动识别 SKILL.md。

触发方式：
- 说"获取今日晨报"
- 说"同步晨间新闻"
- 或直接运行 `/morning-news`（如果配置了命令）
