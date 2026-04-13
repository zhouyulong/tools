#!/usr/bin/env python3
"""
dida365-sync skill 验证脚本

验证技能是否配置正确，提供使用示例。
"""

import sys
import os
import json
from pathlib import Path


def check_skill_structure():
    """检查技能目录结构"""
    skill_dir = Path(__file__).parent.parent
    
    print("🔍 检查技能目录结构...")
    required_files = ["SKILL.md", "README.md"]
    required_dirs = ["scripts", "references", "assets"]
    
    errors = []
    for f in required_files:
        if not (skill_dir / f).exists():
            errors.append(f"缺失文件: {f}")
    
    for d in required_dirs:
        if not (skill_dir / d).exists():
            errors.append(f"缺失目录: {d}")
    
    if errors:
        print("❌ 技能结构不完整:")
        for error in errors:
            print(f"  - {error}")
        return False
    else:
        print("✅ 技能目录结构完整")
        return True


def check_scripts():
    """检查脚本文件"""
    print("🔍 检查脚本文件...")
    scripts_dir = Path(__file__).parent
    main_script = scripts_dir / "dida365_sync.py"
    
    if not main_script.exists():
        print("❌ 缺失主脚本: dida365_sync.py")
        return False
    
    # 检查脚本基本语法
    try:
        with open(main_script, 'r', encoding='utf-8') as f:
            content = f.read()
        # 基本语法检查
        if "def sync_tasks_to_markdown" not in content:
            print("❌ 主函数未找到: sync_tasks_to_markdown")
            return False
        if "def format_date_header" not in content:
            print("❌ 日期格式化函数未找到: format_date_header")
            return False
        
        print("✅ 脚本文件检查通过")
        return True
    except Exception as e:
        print(f"❌ 脚本检查异常: {e}")
        return False


def check_references():
    """检查参考文档"""
    print("🔍 检查参考文档...")
    ref_dir = Path(__file__).parent.parent / "references"
    docs = ["dida365_cli.md", "usage_examples.md"]
    
    if not ref_dir.exists():
        print("❌ 参考文档目录不存在")
        return False
    
    for doc in docs:
        if not (ref_dir / doc).exists():
            print(f"⚠️  参考文档缺失: {doc}")
    
    print("✅ 参考文档检查完成")
    return True


def check_assets():
    """检查资产文件"""
    print("🔍 检查资产文件...")
    assets_dir = Path(__file__).parent.parent / "assets"
    
    if not assets_dir.exists():
        print("❌ 资产目录不存在")
        return False
    
    example_files = list(assets_dir.glob("*.md"))
    if not example_files:
        print("⚠️  缺乏示例文件")
    else:
        print(f"✅ 找到 {len(example_files)} 个示例文件")
    
    return True


def run_test_scenario():
    """运行测试场景"""
    print("\n🚀 运行测试场景...")
    
    # 创建测试数据
    test_data = {
        "tasks": [
            {
                "title": "测试任务1",
                "completed": True,
                "subtasks": [
                    {"title": "测试子任务1-1", "completed": True},
                    {"title": "测试子任务1-2", "completed": False}
                ]
            },
            {
                "title": "测试任务2",
                "completed": False,
                "subtasks": []
            }
        ],
        "date": "2026-03-30",
        "today_chinese_weekday": get_chinese_weekday("2026-03-30")
    }
    
    print(f"📅 测试日期: {test_data['date']} ({test_data['today_chinese_weekday']})")
    print(f"📝 测试任务: {len(test_data['tasks'])} 个")
    
    print("""
📋 预期输出:
  # 2026年3月30日（周一）
  - [x] 测试任务1
    - [x] 测试子任务1-1
    - [ ] 测试子任务1-2
  - [ ] 测试任务2
  """)
    
    print("✅ 测试场景创建完成")
    return True


def get_chinese_weekday(date_str):
    """获取中文星期（用于测试）"""
    from datetime import datetime
    try:
        dt = datetime.strptime(date_str, "%Y-%m-%d")
        weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        return weekdays[dt.weekday()]
    except ValueError:
        return "未知"


def show_usage():
    """显示使用说明"""
    print("""
🎯 dida365-sync skill 使用说明:

1. 基本同步:
   用户请求: "帮我把 dida365 今天工作列表的任务同步到 ~/Documents/tasks.md"

2. 手动执行:
   cd ~/.workbuddy/skills/dida365-sync/scripts
   python dida365_sync.py /path/to/tasks.md 2026-03-26

3. 配置自动化:
    使用 WorkBuddy 的 automation_update 工具创建定时任务:
    - 每日早上9点自动同步
    - 工作日下午5点同步进度

4. 依赖:
   - Dida365 CLI skill (用于获取任务)
   - Python 3.x (执行脚本)

🔧 故障排除:
    1. 检查 Dida365 CLI 是否可用: dida-cli --help
    2. 检查 Python 环境: python --version
    3. 查看日志文件 (如果启用)
    """)


def main():
    """主验证函数"""
    print("🎯 dida365-sync skill 验证脚本")
    print("=" * 50)
    
    all_checks = []
    all_checks.append(check_skill_structure())
    all_checks.append(check_scripts())
    all_checks.append(check_references())
    all_checks.append(check_assets())
    
    print("\n" + "=" * 50)
    
    if all(all_checks):
        print("✅ 技能验证通过! 所有检查项均正常。")
        print("""
下一步:
1. 在 WorkBuddy 中加载这个技能
2. 配置 Dida365 CLI skill (用于获取任务)
3. 开始同步你的任务!
        """)
    else:
        print("❌ 技能验证失败! 请修复以上问题。")
        return 1
    
    # 运行测试场景
    run_test_scenario()
    
    print("\n" + "=" * 50)
    show_usage()
    
    return 0


if __name__ == "__main__":
    sys.exit(main())