# Dida365 CLI 集成指南

本文档说明如何集成 Dida365 CLI 获取任务数据。

## Dida365 CLI 概述

Dida365 CLI 是一个 Node.js 命令行工具，用于管理滴答清单的任务、项目、标签等。

## 常用命令

### 查询工作列表中的任务

```bash
# 查询工作列表中的所有任务
dida-cli list-working

# 查询特定日期的任务
dida-cli list-working --date 2026-03-26

# 查询已完成任务
dida-cli list-working --completed true

# 格式化输出为 JSON
dida-cli list-working --format json
```

### 任务数据结构

从 Dida365 CLI 返回的任务数据结构：

```json
{
  "id": "task_id",
  "title": "任务标题",
  "completed": false,
  "startDate": "2026-03-26",
  "dueDate": "2026-03-26",
  "isWorking": true,
  "subtasks": [
    {
      "id": "subtask_id",
      "title": "子任务标题",
      "completed": false
    }
  ],
  "projectId": "project_id",
  "listId": "working_list_id"
}
```

## 集成到同步脚本

### 步骤 1: 获取今日工作列表任务

使用 WorkBuddy 的 Dida365 CLI skill 获取任务：

```python
# 在 Python 脚本中调用 Dida365 CLI
import subprocess
import json

def get_dida365_working_tasks(date=None):
    """
    从 dida365 获取工作列表中的任务
    
    Args:
        date: 可选的日期字符串 YYYY-MM-DD，不指定则获取所有
    
    Returns:
        任务列表
    """
    cmd = ["dida-cli", "list-working", "--format", "json"]
    if date:
        cmd.extend(["--date", date])
    
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=True
        )
        return json.loads(result.stdout)
    except subprocess.CalledProcessError as e:
        print(f"Dida365 CLI 执行失败: {e}", file=sys.stderr)
        return []
    except json.JSONDecodeError as e:
        print(f"解析 Dida365 输出失败: {e}", file=sys.stderr)
        return []
```

### 步骤 2: 过滤今日任务

```python
from datetime import datetime

def filter_today_tasks(tasks, date_str):
    """
    过滤出指定日期的任务
    
    Args:
        tasks: dida365 任务列表
        date_str: 日期字符串 YYYY-MM-DD
    
    Returns:
        今日任务列表
    """
    today_tasks = []
    
    for task in tasks:
        # 检查任务是否为工作列表中的任务
        if not task.get("isWorking"):
            continue
        
        # 检查任务日期
        task_date = task.get("startDate") or task.get("dueDate")
        if task_date == date_str:
            today_tasks.append(task)
    
    return today_tasks
```

### 步骤 3: 转换为同步脚本需要的格式

```python
def convert_dida_to_sync_format(dida_tasks):
    """
    将 dida365 任务格式转换为同步脚本需要的格式
    
    Args:
        dida_tasks: dida365 任务列表
    
    Returns:
        转换后的任务列表
    """
    sync_tasks = []
    
    for task in dida_tasks:
        sync_task = {
            "title": task["title"],
            "completed": task["completed"],
            "subtasks": []
        }
        
        # 添加子任务
        if task.get("subtasks"):
            for subtask in task["subtasks"]:
                sync_task["subtasks"].append({
                    "title": subtask["title"],
                    "completed": subtask["completed"]
                })
        
        sync_tasks.append(sync_task)
    
    return sync_tasks
```

## 完整集成示例

```python
def main():
    import sys
    from datetime import datetime
    
    # 获取今日日期
    today = datetime.now().strftime("%Y-%m-%d")
    
    # 获取 dida365 任务
    dida_tasks = get_dida365_working_tasks(today)
    if not dida_tasks:
        print("今日没有任务", file=sys.stderr)
        return
    
    # 转换格式
    sync_tasks = convert_dida_to_sync_format(dida_tasks)
    
    # 同步到 Markdown
    markdown_path = sys.argv[1]
    new_content = sync_tasks_to_markdown(sync_tasks, markdown_path, today)
    
    # 写入文件
    with open(markdown_path, 'w', encoding='utf-8') as f:
        f.write(new_content)
    
    print(f"成功同步 {len(sync_tasks)} 个任务到 {markdown_path}")
```

## 注意事项

1. **认证**: Dida365 CLI 需要先配置认证信息（API Key 或登录凭证）
2. **错误处理**: 需要处理网络错误、认证失败等情况
3. **日期格式**: 确保日期格式一致，使用 YYYY-MM-DD 格式
4. **工作列表识别**: 通过 `isWorking` 字段识别工作列表中的任务
5. **子任务处理**: 确保子任务的完成状态也能正确同步

## 在 WorkBuddy 中使用

当用户要求同步任务时：

1. 加载 Dida365 CLI skill
2. 调用 CLI 获取工作列表任务
3. 使用本技能的脚本同步到 Markdown
4. 报告同步结果和变更内容

---

*更多 Dida365 CLI 的详细信息，请参考 Dida365 CLI skill 的文档。*
