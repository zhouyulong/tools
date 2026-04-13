#!/usr/bin/env python3
"""
dida365 working 项目任务同步到 Markdown 文档

用法:
    python dida365_working_sync.py <date>
    
参数:
    date: 目标日期，格式为 YYYY-MM-DD (例如: 2026-03-31)
    默认为今天

固定配置:
    - 项目: working (ID: 1737466f9955964c71817638)
    - 基础目录: D:\CodeAndNotes\records\02-TaskNotes\OS
    - 文件名格式: WorkNotes-YYYYMM-YYYYMM.md (双月制)
"""

import sys
import re
import json
import subprocess
from datetime import datetime, timedelta
from pathlib import Path

# ==================== 可配置区域 ====================
# 基础目录（必须已存在，可修改此路径）
BASE_DIR = r"D:\CodeAndNotes\records\02-TaskNotes\OS"

# working 项目 ID（固定）
WORKING_PROJECT_ID = "1737466f9955964c71817638"
# ==================================================


def get_target_filename(date_obj):
    """根据日期生成目标文件名（双月制）
    
    格式: WorkNotes-YYYYMM-MM.md
    例如: 2026年3月 -> WorkNotes-202603-04.md
    """
    year = date_obj.year
    month = date_obj.month
    
    # 双月制分组
    if month in [1, 2]:
        start_month, end_month = 1, 2
    elif month in [3, 4]:
        start_month, end_month = 3, 4
    elif month in [5, 6]:
        start_month, end_month = 5, 6
    elif month in [7, 8]:
        start_month, end_month = 7, 8
    elif month in [9, 10]:
        start_month, end_month = 9, 10
    else:
        start_month, end_month = 11, 12
    
    return f"WorkNotes-{year}{start_month:02d}-{end_month:02d}.md"


def get_target_file_path(date_obj):
    """获取完整目标文件路径"""
    filename = get_target_filename(date_obj)
    return Path(BASE_DIR) / filename


def check_base_directory():
    """检查基础目录是否存在"""
    base_path = Path(BASE_DIR)
    if not base_path.exists():
        print(f"[错误] 基础目录不存在")
        print(f"   路径: {BASE_DIR}")
        print(f"\n请确认:")
        print(f"   1. 目录路径是否正确")
        print(f"   2. 如需更改路径，请修改脚本中的 BASE_DIR 变量")
        return False
    return True


def check_target_file(date_obj):
    """检查目标文件是否存在，不存在则提示用户"""
    target_path = get_target_file_path(date_obj)
    
    if target_path.exists():
        return True, target_path
    
    print(f"[错误] 目标文件不存在")
    print(f"   路径: {target_path}")
    print(f"\n请选择操作:")
    print(f"   1. 手动创建该文件")
    print(f"   2. 授权脚本自动创建空文件")
    print(f"   3. 退出并检查配置")
    
    # 在交互式环境中询问用户
    try:
        choice = input("\n请输入选项 (1/2/3): ").strip()
    except EOFError:
        # 非交互式环境，默认退出
        print("\n非交互式环境，请手动创建文件后重试")
        return False, target_path
    
    if choice == "1":
        print(f"\n请手动创建文件: {target_path}")
        print("创建完成后请重新运行脚本")
        return False, target_path
    elif choice == "2":
        try:
            target_path.touch()
            print(f"[成功] 已创建空文件: {target_path}")
            return True, target_path
        except Exception as e:
            print(f"[错误] 创建文件失败: {e}")
            return False, target_path
    else:
        print("已退出")
        return False, target_path


def run_dida365_command(args):
    """运行 dida365 CLI 命令并返回解析后的 JSON"""
    try:
        # 使用 PowerShell 执行 npx 命令
        cmd = ["powershell.exe", "-Command", f"cd C:\\Users\\yl-c\\WorkBuddy\\20260331085842; npx dida365 {' '.join(args)} --json 2>&1"]
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            encoding='utf-8'
        )
        
        # 清理 CLIXML 输出
        output = result.stdout
        if output.startswith('#< CLIXML'):
            output = output.split('\n', 1)[1] if '\n' in output else output
        
        # 移除 PowerShell 进度信息
        if '<Objs Version=' in output:
            output = output.split('<Objs Version=')[0].strip()
        
        return json.loads(output)
    except Exception as e:
        print(f"命令执行失败: {e}", file=sys.stderr)
        return None


def get_today_pending_tasks(date_str):
    """获取今日待完成任务"""
    data = run_dida365_command(["sync", "all", "--json"])
    if not data or "tasks" not in data:
        return []
    
    target_date = date_str  # 格式: 2026-03-31
    
    tasks = []
    for task in data["tasks"]:
        # 筛选 working 项目的任务
        if task.get("projectId") != WORKING_PROJECT_ID:
            continue
        
        due_date = task.get("dueDate", "")
        # 将 dueDate 转换为北京时间 (dueDate格式: 2026-03-31T09:00:00.000+0000)
        due_date_part = ""
        if due_date:
            try:
                # 手动解析日期和时间 (兼容带毫秒的格式)
                # 格式: 2026-03-31T16:00:00.000+0000
                date_str = due_date[:19]  # 取到秒: 2026-03-31T16:00:00
                tz_offset = due_date[23:25]  # 取时区偏移小时: +00
                dt = datetime.strptime(date_str, "%Y-%m-%dT%H:%M:%S")
                # 转换为北京时间 (UTC+8)，滴答清单返回的是 UTC 时间
                beijing_dt = dt + timedelta(hours=8)
                due_date_part = beijing_dt.strftime("%Y-%m-%d")
            except (ValueError, IndexError):
                # 解析失败时回退到原逻辑
                due_date_part = due_date[:10] if len(due_date) >= 10 else ""
        
        # status == 0 表示未完成
        if due_date_part == target_date and task.get("status") == 0:
            tasks.append({
                "title": task["title"],
                "completed": False,
                "subtasks": [
                    {"title": item["title"], "completed": item.get("status") == 1}
                    for item in task.get("items", [])
                ]
            })
    
    return tasks


def get_today_completed_tasks():
    """获取今日已完成任务"""
    data = run_dida365_command(["completed", "today", "--json"])
    if not data:
        return []
    
    tasks = []
    for task in data:
        if task.get("projectId") == WORKING_PROJECT_ID:
            tasks.append({
                "title": task["title"],
                "completed": True,
                "subtasks": [
                    {"title": item["title"], "completed": item.get("status") == 1}
                    for item in task.get("items", [])
                ]
            })
    
    return tasks


def get_weekday_chinese(date_str):
    """将日期字符串转换为中文星期"""
    try:
        dt = datetime.strptime(date_str, "%Y-%m-%d")
        weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        return weekdays[dt.weekday()]
    except ValueError:
        return None


def format_date_header(date_str):
    """格式化日期标题"""
    try:
        dt = datetime.strptime(date_str, "%Y-%m-%d")
        weekday = get_weekday_chinese(date_str)
        return f"# {dt.year}年{dt.month}月{dt.day}日（{weekday}）"
    except ValueError:
        return None


def format_task_markdown(task, indent=0):
    """格式化任务为 Markdown"""
    prefix = "  " * (indent // 2)
    status = "x" if task["completed"] else " "
    result = f"{prefix}- [{status}] {task['title']}\n"
    
    for subtask in task.get("subtasks", []):
        result += format_task_markdown(subtask, indent + 2)
    
    return result


def read_target_file(target_path):
    """读取目标文件内容"""
    try:
        with open(target_path, 'r', encoding='utf-8') as f:
            return f.read()
    except FileNotFoundError:
        return ""
    except Exception as e:
        print(f"读取文件失败: {e}", file=sys.stderr)
        return None


def write_target_file(target_path, content):
    """写入目标文件"""
    try:
        # 备份原文件
        backup_path = str(target_path) + ".bak"
        if target_path.exists():
            target_path.rename(backup_path)
        
        with open(target_path, 'w', encoding='utf-8') as f:
            f.write(content)
        
        # 删除备份
        if Path(backup_path).exists():
            Path(backup_path).unlink()
        
        return True
    except Exception as e:
        print(f"写入文件失败: {e}", file=sys.stderr)
        # 恢复备份
        if Path(backup_path).exists():
            Path(backup_path).rename(target_path)
        return False


def parse_task_line(line):
    """解析任务行，返回 (is_task, is_completed, title, indent_level)"""
    match = re.match(r'^(\s*)- \[([ x])\] (.+)$', line)
    if match:
        indent = len(match.group(1)) // 2
        completed = match.group(2) == 'x'
        title = match.group(3).strip()
        return True, completed, title, indent
    return False, False, None, 0


def find_date_section(content, date_header):
    """查找日期区块，返回 (before, section_content, after)"""
    # 查找日期标题
    pattern = rf"^{re.escape(date_header)}$"
    match = re.search(pattern, content, re.MULTILINE)
    
    if not match:
        return content, None, ""
    
    start = match.start()
    # 查找下一个一级标题
    next_match = re.search(r"^# ", content[match.end():], re.MULTILINE)
    if next_match:
        end = match.end() + next_match.start()
    else:
        end = len(content)
    
    return content[:start], content[start:end], content[end:]


def find_tasks_in_section(section_content):
    """在日期区块中查找任务列表，返回 (before, tasks_content, after)"""
    # 查找第一个任务行（- [ ] 或 - [x]）
    match = re.search(r"^\s*- \[[ x]\] ", section_content, re.MULTILINE)
    if not match:
        return section_content, None, ""
    
    start = match.start()
    # 查找下一个一级标题或二级标题
    next_match = re.search(r"^#+ ", section_content[start:], re.MULTILINE)
    if next_match:
        end = start + next_match.start()
    else:
        end = len(section_content)
    
    return section_content[:start], section_content[start:end], section_content[end:]





def parse_existing_tasks(status_content):
    """解析现有任务，返回任务字典 {title: (is_completed, subtasks)}"""
    tasks = {}
    current_task = None
    current_subtasks = []
    
    lines = status_content.split('\n') if status_content else []
    for line in lines:
        is_task, completed, title, indent = parse_task_line(line)
        if is_task:
            if indent == 0:
                # 保存上一个任务
                if current_task:
                    tasks[current_task] = (current_completed, current_subtasks)
                # 开始新任务
                current_task = title
                current_completed = completed
                current_subtasks = []
            else:
                # 子任务
                if current_task is not None:
                    current_subtasks.append((title, completed))
    
    # 保存最后一个任务
    if current_task:
        tasks[current_task] = (current_completed, current_subtasks)
    
    return tasks


def build_tasks_section(tasks):
    """构建任务列表内容（平铺格式，无二级三级目录）"""
    if not tasks:
        return ""
    
    content = ""
    for task in tasks:
        status = "x" if task["completed"] else " "
        content += f"- [{status}] {task['title']}\n"
        for subtask in task.get("subtasks", []):
            sub_status = "x" if subtask["completed"] else " "
            content += f"  - [{sub_status}] {subtask['title']}\n"
    content += "\n"
    return content


def merge_tasks(existing_tasks, new_tasks):
    """合并现有任务和新任务，返回更新后的任务列表"""
    # 创建新任务字典（用于快速查找）
    new_task_dict = {t["title"]: t for t in new_tasks}
    
    merged = []
    updated_count = 0
    added_count = 0
    
    # 首先处理现有任务
    for title, (completed, subtasks) in existing_tasks.items():
        if title in new_task_dict:
            # 任务存在，更新状态
            new_task = new_task_dict[title]
            if completed != new_task["completed"]:
                updated_count += 1
            merged.append(new_task)
            del new_task_dict[title]
        else:
            # 任务在滴答清单中不存在，保留但标记为可能已删除
            task = {
                "title": title,
                "completed": completed,
                "subtasks": [{"title": st[0], "completed": st[1]} for st in subtasks]
            }
            merged.append(task)
    
    # 添加新任务
    for title, task in new_task_dict.items():
        merged.append(task)
        added_count += 1
    
    return merged, updated_count, added_count


def sync_working_tasks(date_str, target_path):
    """同步 working 项目任务（智能更新，平铺格式）"""
    print(f"正在同步 working 项目 {date_str} 的任务...")
    
    # 获取任务
    pending_tasks = get_today_pending_tasks(date_str)
    completed_tasks = get_today_completed_tasks()
    
    print(f"  滴答清单 - 待完成: {len(pending_tasks)} 个, 已完成: {len(completed_tasks)} 个")
    
    # 生成日期标题
    date_header = format_date_header(date_str)
    if not date_header:
        print(f"无效的日期格式: {date_str}", file=sys.stderr)
        return False
    
    # 读取现有文件
    content = read_target_file(target_path)
    if content is None:
        return False
    
    # 查找日期区块
    before, date_section, after = find_date_section(content, date_header)
    
    if date_section is None:
        # 创建新的日期区块
        print("  创建新的日期区块")
        all_tasks = pending_tasks + completed_tasks
        tasks_section = build_tasks_section(all_tasks)
        
        new_section = f"{date_header}\n\n{tasks_section}"
        # 追加到之前内容后面（before 包含文件所有内容）
        new_content = before + "\n\n" + new_section
    else:
        # 更新现有日期区块
        print("  更新现有日期区块")
        date_before, tasks_section, date_after = find_tasks_in_section(date_section)
        
        if tasks_section is None:
            # 添加任务列表
            print("  添加任务列表")
            all_tasks = pending_tasks + completed_tasks
            tasks_content = build_tasks_section(all_tasks)
            new_section = date_section.rstrip() + "\n\n" + tasks_content
        else:
            # 更新任务列表
            print("  更新任务列表")
            # 解析现有任务（不区分待完成/已完成，统一处理）
            existing_tasks = parse_existing_tasks(tasks_section)
            
            # 合并所有任务
            all_new_tasks = pending_tasks + completed_tasks
            merged_tasks, updated_count, added_count = merge_tasks(existing_tasks, all_new_tasks)
            
            if updated_count > 0:
                print(f"  更新任务状态: {updated_count} 个")
            if added_count > 0:
                print(f"  新增任务: {added_count} 个")
            
            # 构建新的任务列表（按状态排序：待完成在前，已完成在后）
            sorted_tasks = sorted(merged_tasks, key=lambda t: t["completed"])
            new_tasks_content = build_tasks_section(sorted_tasks)
            
            # 替换任务列表
            new_section = date_before + new_tasks_content + date_after
        
        new_content = before + new_section + after
    
    # 写入文件
    if write_target_file(target_path, new_content):
        print(f"[完成] 同步完成: {target_path}")
        return True
    else:
        return False


def main():
    """主函数"""
    # 获取日期参数，默认为今天
    if len(sys.argv) >= 2:
        date_str = sys.argv[1]
        try:
            date_obj = datetime.strptime(date_str, "%Y-%m-%d")
        except ValueError:
            print(f"无效的日期格式: {date_str}", file=sys.stderr)
            print("请使用格式: YYYY-MM-DD", file=sys.stderr)
            sys.exit(1)
    else:
        date_obj = datetime.now()
        date_str = date_obj.strftime("%Y-%m-%d")
    
    print(f"[日期] 目标日期: {date_str}")
    print(f"[目录] 基础目录: {BASE_DIR}")
    
    # 检查基础目录
    if not check_base_directory():
        sys.exit(1)
    
    # 检查目标文件
    file_exists, target_path = check_target_file(date_obj)
    if not file_exists:
        sys.exit(1)
    
    print(f"[文件] 目标文件: {target_path}")
    print()
    
    # 同步任务
    success = sync_working_tasks(date_str, target_path)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
