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
  "ANTHROPIC_BASE_URL": "https://your-api-endpoint.com/v1",
  "model": "kimi-model-name"
}
```

## 安装

将项目克隆到本地并添加到 PATH：

```bash
git clone <repo-url> ~/personaltools
echo 'export PATH="$HOME/personaltools/claude:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## 贡献

欢迎提交 Issue 和 PR 添加新工具或改进现有工具。

## License

MIT
