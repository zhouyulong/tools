#!/usr/bin/env python3
"""
dida365-sync 自动化配置脚本

创建定时同步任务的 automation 配置。
"""

import json
import sys
from pathlib import Path


def create_daily_sync_automation():
    """创建每日同步任务"""
    automation = {
        "mode": "suggested create",
        "name": "dida365 每日任务同步",
        "prompt": "将 dida365 工作列表中的今日任务同步到指定 Markdown 文档。具体操作：1. 查询 dida365 今日工作列表任务；2. 读取本地 Markdown 文档；3. 更新或添加今日任务，保留其他内容；4. 保存文档。",
        "cwds": json.dumps([str(Path.home() / "Documents")]),
        "status": "ACTIVE",
        "scheduleType": "recurring",
        "rrule": "FREQ=DAILY;BYHOUR=9;BYMINUTE=0",
        "maxDurationMinutes": 10,
        "validFrom": "2026-03-30",
        "validUntil": None  # 无限期
    }
    
    print("🔄 创建的 automation 配置:")
    for key, value in automation.items():
        if value:
            print(f"  {key}: {value}")
    
    print("\n📝 WorkBuddy 调用示例:")
    print(f"""
    await tools.automation_update({{
        mode: '{automation["mode"]}',
        name: '{automation["name"]}',
        prompt: '{automation["prompt"]}',
        cwds: '{automation["cwds"]}',
        status: '{automation["status"]}',
        scheduleType: '{automation["scheduleType"]}',
        rrule: '{automation["rrule"]}',
        maxDurationMinutes: {automation["maxDurationMinutes"]},
        validFrom: '{automation["validFrom"]}'
    }})
    """)
    
    return automation


def create_workday_sync_automation():
    """创建工作日同步任务（早晚各一次）"""
    automations = []
    
    # 早上同步
    morning = {
        "mode": "suggested create",
        "name": "dida365 早间任务同步",
        "prompt": "在每日工作开始时同步 dida365 任务，查看今日计划。具体操作：1. 查询 dida365 今日工作列表任务；2. 读取本地任务文档；3. 更新今天的工作计划；4. 输出今日待办摘要。",
        "cwds": json.dumps([str(Path.home() / "Documents")]),
        "status": "ACTIVE",
        "scheduleType": "recurring",
        "rrule": "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=9;BYMINUTE=0",
        "maxDurationMinutes": 5,
        "validFrom": "2026-03-30"
    }
    
    # 晚上同步
    evening = {
        "mode": "suggested create",
        "name": "dida365 晚间任务同步",
        "prompt": "在每日工作结束时同步 dida365 任务，更新完成状态。具体操作：1. 查询 dida365 今日工作列表任务；2. 读取本地任务文档；3. 更新任务的完成状态；4. 输出今日工作完成情况汇报。",
        "cwds": json.dumps([str(Path.home() / "Documents")]),
        "status": "ACTIVE",
        "scheduleType": "recurring",
        "rrule": "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=17;BYMINUTE=30",
        "maxDurationMinutes": 5,
        "validFrom": "2026-03-30"
    }
    
    automations.append(morning)
    automations.append(evening)
    
    print("🏢 创建工作日报步配置:")
    for i, auto in enumerate(automations, 1):
        print(f"\n{i}. {auto['name']}:")
        print(f"   时间: {auto['rrule']}")
        print(f"   目的: {auto['prompt'][:80]}...")
    
    return automations


def create_one_time_sync():
    """创建一次性同步任务模板"""
    automation = {
        "mode": "suggested create",
        "name": "dida365 手动任务同步",
        "prompt": "立即同步 dida365 今日任务到指定 Markdown 文档。具体操作：1. 查询 dida365 今日工作列表任务；2. 读取本地任务文档；3. 更新或添加任务；4. 保存文档；5. 显示变更摘要。",
        "cwds": json.dumps([str(Path.home() / "Documents")]),
        "status": "PAUSED",  # 手动触发
        "scheduleType": "once",
        "scheduledAt": "2026-03-30T10:00:00"
    }
    
    print("🔧 一次性任务模板:")
    print("适用于临时手动同步场景")
    print("需要时修改 scheduledAt 和 status 为 ACTIVE")
    
    return automation


def main():
    """主函数"""
    print("🤖 dida365 任务同步自动化配置")
    print("=" * 60)
    
    print("""
请选择配置类型:
1. 每日自动同步 (推荐)
2. 工作日早晚同步 (办公场景)
3. 一次性手动同步
4. 查看所有配置
    """)
    
    try:
        choice = input("请输入选择 (1-4): ").strip()
    except (KeyboardInterrupt, EOFError):
        print("\n操作取消")
        return 0
    
    print("\n" + "=" * 60)
    
    if choice == "1":
        create_daily_sync_automation()
    elif choice == "2":
        create_workday_sync_automation()
    elif choice == "3":
        create_one_time_sync()
    elif choice == "4":
        print("📋 所有可用配置:")
        print("=" * 40)
        create_daily_sync_automation()
        print("\n" + "=" * 40)
        create_workday_sync_automation()
        print("\n" + "=" * 40)
        create_one_time_sync()
    else:
        print("❌ 无效选择")
        return 1
    
    print("\n" + "=" * 60)
    print("""
🎯 使用说明:
1. 复制上面的 JSON 配置
2. 在 WorkBuddy 中使用 automation_update 工具
3. 如果需要，根据实际情况修改:
   - cwds: 工作目录
   - rrule: 执行频率
   - maxDurationMinutes: 任务执行时间限制
   - validFrom/validUntil: 有效期

📋 示例对话:
用户: "帮我创建一个每天自动同步 dida365 任务的功能"
助手: (使用上述配置创建 automation)

🔧 自定义提示 (prompt) 建议:
- 详细描述同步的具体步骤
- 指定目标文件路径或变量
- 包含错误处理逻辑
- 输出格式要求
    """)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())