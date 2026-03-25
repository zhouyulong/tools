# Personal Tools

个人开发的小工具集合，用于提升日常开发效率。

## 工具列表

### Claude 工具集 (`claude/`)

#### claudemodel

快速切换 Claude Code 使用的 AI 模型。通过替换 `~/.claude/settings.json` 配置文件实现模型切换。

**使用方法：**

```bash
# 切换到指定模型
claudemodel <model-name>

# 查看当前使用的模型
claudemodel current

# 显示帮助信息
claudemodel -h
```

**配置要求：**

需要在 `~/.claude/` 目录下预先创建各模型的配置文件，命名格式为 `settings.json.<model-name>`。

例如：
- `~/.claude/settings.json.kimi` - Kimi 模型配置
- `~/.claude/settings.json.claude` - Claude 官方模型配置

**示例配置文件内容：**

```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-kimi-xxxxx",
    "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding",
    "API_TIMEOUT_MS": "3000000",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "kimi-2.5"
  },
  "model": "opus[1m]",
  "alwaysThinkingEnabled": false
}
```

## 安装

将项目克隆到本地并添加到 PATH：

```bash
git clone <repo-url> ~/personaltools
echo 'export PATH="$HOME/personaltools/claude:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## License

MIT
