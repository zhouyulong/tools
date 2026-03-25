#!/bin/bash

# 切换claude接入模型
# 通过快速修改.claude/setting.json内容实现
# 帮助：claudemode.sh -h
#



show_help() {
    echo "claudemodel一键切换模型"
    cat << HELP
     使用方法:
         claudemodel [modelname] 切换到指定模型
         claudemodel -h,--help   显示帮助信息
	 claudemodel current     查看当前使用模型名称
     示例：
         claudemodel kimi
         claudemodel current
HELP
}

# 无参数 or 帮助
#
if [ $# -eq 0 ] || [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
	show_help
	exit 0
fi

# 查看当前配置

if [ "$1" = "current" ]; then
	echo -e "\n 当前Claude配置:"
	echo -E "ANTHROPIC_BASE_URL|model" ~/.claude/setting.json
	echo ""
	exit 0
fi

# 切换模型

MODEL="$1"
CONFIG="$HOME/.claude/settings.json.$MODEL"

if [ ! -f "$CONFIG" ]; then
    echo "X 模型 $MODEL 不存在配置文件：$CONFIG，请先创建该文件并进行基本配置"
    exit 1
fi

cp "$CONFIG" ~/.claude/settings.json
echo "ClaudeCode已经切换到模型 $MODEL"


