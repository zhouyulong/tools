#!/usr/bin/env python3
"""
dida365 任务同步到 Markdown 文档

用法:
    python dida365_sync.py <markdown_path> <date>
    
参数:
    markdown_path: Markdown 文档的路径
    date: 目标日期，格式为 YYYY-MM-DD (例如: 2026-03-26)
"""

import sys
import re
import json
from datetime import datetime
from pathlib import Path


def get_weekday_chinese(date_str):
    """
    将日期字符串转换为中文星期
    
    Args:
        date_str: 日期字符串，格式为 YYYY-MM-DD
    
    Returns:
        中文星期几 (例如: "周一", "周二")
    """
    try:
        dt = datetime.strptime(date_str, "%Y-%m-%d")
        weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        return weekdays[dt.weekday()]
    except ValueError:
        return None


def format_date_header(date_str):
    """
    格式化日期标题
    
    Args:
        date_str: 日期字符串，格式为 YYYY-MM-DD
    
    Returns:
        格式化后的日期标题 (例如: "# 2026年3月26日（周四）")
    """
    try:
        dt = datetime.strptime(date_str, "%Y-%m-%d")
        weekday = get_weekday_chinese(date_str)
        # 格式: 2026年3月26日（周四）
        # 注意：如果月份或日期是单数，不补零
        month = dt.month
        day = dt.day
        return f"# {dt.year}年{month}月{day}日（{weekday}）"
    except ValueError:
        return None


def parse_markdown_tasks(content, date_header):
    """
    解析 Markdown 文档中的任务
    
    Args:
        content: Markdown 文档内容
        date_header: 要查找的日期标题
    
    Returns:
        {
            "start_pos": 日期标题在文档中的起始位置,
            "end_pos": 日期标题部分的结束位置,
            "tasks": [
                {"name": "任务名", "completed": True/False, "subtasks": [...]}
            ],
            "before": 日期标题之前的内容,
            "after": 日期标题之后的内容
        }
    """
    # 查找日期标题
    date_pattern = re.escape(date_header)
    date_match = re.search(rf"^# {date_pattern}$", content, re.MULTILINE)
    
    if not date_match:
        return None
    
    start_pos = date_match.start()
    
    # 查找下一个日期标题（表示当前日期部分的结束）
    # 下一个标题行是以 "# " 开头的
    next_header_match = re.search(r"^# ", content[date_match.end():], re.MULTILINE)
    
    if next_header_match:
        end_pos = date_match.end() + next_header_match.start()
    else:
        end_pos = len(content)
    
    # 提取日期部分
    date_section = content[date_match.start():end_pos]
    
    # 解析任务
    tasks = []
    lines = date_section.split('\n')
    
    for line in lines:
        # 跳过标题行和空行
        if line.startswith('#') or not line.strip():
            continue
        
        # 解析主任务
        task_match = re.match(r'^(\s*)- \[(x| )\]\s*(.+)$', line)
        if task_match:
            indent = len(task_match.group(1))
            completed = task_match.group(2) == 'x'
            task_name = task_match.group(3)
            
            # 如果是主任务（无缩进或缩进为2空格，根据上下文判断）
            # 这里简单处理：缩进为0的是主任务
            if indent == 0:
                tasks.append({
                    "name": task_name,
                    "completed": completed,
                    "subtasks": []
                })
            # 子任务（缩进为2空格）
            elif indent == 2 and tasks:
                tasks[-1]["subtasks"].append({
                    "name": task_name,
                    "completed": completed
                })
    
    return {
        "start_pos": start_pos,
        "end_pos": end_pos,
        "tasks": tasks,
        "before": content[:start_pos],
        "after": content[end_pos:] if end_pos < len(content) else ""
    }


def format_task(task, indent=0):
    """
    格式化任务为 Markdown
    
    Args:
        task: 任务字典 {"name": "...", "completed": True/False, "subtasks": [...]}
        indent: 缩进级别（主任务为0，子任务为2）
    
    Returns:
        Markdown 格式的任务字符串
    """
    prefix = " " * indent
    status = "x" if task["completed"] else " "
    result = f"{prefix}- [{status}] {task['name']}\n"
    
    for subtask in task.get("subtasks", []):
        result += format_task(subtask, indent + 2)
    
    return result


def sync_tasks_to_markdown(dida_tasks, markdown_path, date_str):
    """
    同步 dida365 任务到 Markdown 文档
    
    Args:
        dida_tasks: dida365 任务列表 [{"title": "...", "completed": True/False, "subtasks": [...]}]
        markdown_path: Markdown 文档路径
        date_str: 日期字符串 YYYY-MM-DD
    
    Returns:
        更新后的文档内容
    """
    # 读取现有文档
    try:
        with open(markdown_path, 'r', encoding='utf-8') as f:
            content = f.read()
    except FileNotFoundError:
        content = ""
    except Exception as e:
        print(f"读取文件失败: {e}", file=sys.stderr)
        sys.exit(1)
    
    # 生成日期标题
    date_header = format_date_header(date_str)
    if not date_header:
        print(f"无效的日期格式: {date_str}", file=sys.stderr)
        sys.exit(1)
    
    # 解析现有文档
    parsed = parse_markdown_tasks(content, date_header)
    
    # 创建任务映射（用于快速查找）
    dida_tasks_map = {task["title"]: task for task in dida_tasks}
    
    if parsed:
        # 日期标题已存在，更新任务
        existing_tasks_map = {task["name"]: task for task in parsed["tasks"]}
        
        # 更新现有任务状态
        for task in parsed["tasks"]:
            if task["name"] in dida_tasks_map:
                dida_task = dida_tasks_map[task["name"]]
                task["completed"] = dida_task["completed"]
                
                # 更新子任务
                if dida_task.get("subtasks"):
                    dida_subtasks_map = {st["title"]: st for st in dida_task["subtasks"]}
                    for subtask in task["subtasks"]:
                        if subtask["name"] in dida_subtasks_map:
                            subtask["completed"] = dida_subtasks_map[subtask["name"]]["completed"]
        
        # 添加新任务（存在于 dida 但不存在于本地文档的）
        for dida_task in dida_tasks:
            if dida_task["title"] not in existing_tasks_map:
                new_task = {
                    "name": dida_task["title"],
                    "completed": dida_task["completed"],
                    "subtasks": []
                }
                
                if dida_task.get("subtasks"):
                    for subtask in dida_task["subtasks"]:
                        new_task["subtasks"].append({
                            "name": subtask["title"],
                            "completed": subtask["completed"]
                        })
                
                parsed["tasks"].append(new_task)
        
        # 重新构建文档
        new_content = parsed["before"]
        new_content += f"{date_header}\n"
        
        for task in parsed["tasks"]:
            new_content += format_task(task)
        
        new_content += parsed["after"]
    else:
        # 日期标题不存在，创建新的日期部分
        new_tasks = []
        for dida_task in dida_tasks:
            task = {
                "name": dida_task["title"],
                "completed": dida_task["completed"],
                "subtasks": []
            }
            
            if dida_task.get("subtasks"):
                for subtask in dida_task["subtasks"]:
                    task["subtasks"].append({
                        "name": subtask["title"],
                        "completed": subtask["completed"]
                    })
            
            new_tasks.append(task)
        
        # 将新日期部分添加到文档开头
        new_section = f"{date_header}\n"
        for task in new_tasks:
            new_section += format_task(task)
        
        if content:
            new_content = new_section + "\n" + content
        else:
            new_content = new_section
    
    return new_content


def main():
    """主函数"""
    if len(sys.argv) != 3:
        print(f"用法: {sys.argv[0]} <markdown_path> <date>", file=sys.stderr)
        print("示例: python dida365_sync.py /path/to/tasks.md 2026-03-26", file=sys.stderr)
        sys.exit(1)
    
    markdown_path = sys.argv[1]
    date_str = sys.argv[2]
    
    # 这里应该是从 dida365 CLI 获取任务
    # 暂时使用模拟数据
    # TODO: 集成 Dida365 CLI
    dida_tasks = [
        {
            "title": "安装新版iso",
            "completed": True,
            "subtasks": [
                {"title": "本地", "completed": True},
                {"title": "151机器", "completed": True}
            ]
        },
        {
            "title": "继续集成测试",
            "completed": False,
            "subtasks": []
        }
    ]
    
    # 同步任务
    new_content = sync_tasks_to_markdown(dida_tasks, markdown_path, date_str)
    
    # 写入文件
    try:
        with open(markdown_path, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"任务同步完成: {markdown_path}")
    except Exception as e:
        print(f"写入文件失败: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
