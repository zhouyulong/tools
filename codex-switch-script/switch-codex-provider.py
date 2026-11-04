#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
switch-codex-provider.py —— Codex 多供应商/模型 一键切换（macOS 版）

对应 Windows 版：switch-codex-provider.ps1
原理相同：改写 ~/.codex/config.toml 的根键 model_provider / model，并同步环境变量。

Windows -> macOS 移植要点：
  1. 环境变量：Windows 的「用户级环境变量」在 macOS 不存在，改为
       - 写入 shell 启动文件的托管块（~/.zshrc / ~/.bash_profile / ~/.bashrc）
       - 同步 launchctl setenv / unsetenv，让 Dock、Finder 启动的 GUI 应用也能拿到
  2. 路径：catalog 写入 TOML 时统一正斜杠（macOS 天然如此，仍做规范化）
  3. 交互检测：$env:UserInteractive -> sys.stdin.isatty()
  4. 不再探测 python / py -3 / python3 多候选——sqlite 逻辑由当前解释器直接执行
  5. 读写统一 UTF-8 无 BOM（Codex 是 Rust，带 BOM 会解析失败）
  6. 编码问题无需处理（macOS 终端默认 UTF-8）

用法：
  ./switch-codex-provider.py -L                             # 列出所有供应商（* 为当前生效）
  ./switch-codex-provider.py -P sharedchat                  # 切换：自动探测模型并交互选择（选中即回写默认模型）
  ./switch-codex-provider.py -P sharedchat -M gpt-5.6-sol   # 显式指定模型，跳过菜单（不回写默认模型）
  ./switch-codex-provider.py -P sharedchat --auto           # 直接用默认模型，跳过菜单（脚本化场景）
  ./switch-codex-provider.py -Pr sharedchat                 # 只探测并列出可用模型，不切换
  ./switch-codex-provider.py -P official                    # 切回 OpenAI 官方
  ./switch-codex-provider.py -S                             # 查看当前生效配置 + 环境变量残留检查
  ./switch-codex-provider.py -A myrelay --base-url "https://xxx.com/v1" --api-key "sk-xxx" -M gpt-5.6-sol
  ./switch-codex-provider.py -R myrelay                     # 删除供应商（official 不可删）
  ./switch-codex-provider.py -F "关键词"                     # 按关键词搜索会话
  ./switch-codex-provider.py -Se <session_id|路径> [-P id] [-M 模型]   # 改写会话钉死的供应商

供应商清单：优先脚本同目录 providers.json，否则 ~/.codex/providers.json，可直接手动编辑。
"""

import argparse
import copy
import json
import os
import re
import shlex
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import urllib.request
from datetime import datetime

# ---------------------------------------------------------------- 基础常量

HOME = os.path.expanduser("~")
CODEX_DIR = os.path.join(HOME, ".codex")
CODEX_CONFIG = os.path.join(CODEX_DIR, "config.toml")
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

# 供应商清单：脚本同目录优先，其次 ~/.codex/providers.json
_prov_script = os.path.join(SCRIPT_DIR, "providers.json")
PROV_FILE = _prov_script if os.path.exists(_prov_script) else os.path.join(CODEX_DIR, "providers.json")

OFFICIAL_ID = "official"       # 内置官方供应商 id（providers.json 键名）
OPENAI_BUILTIN = "openai"      # Codex 内置 openai provider 的真实 id（config/rollout 里用的值）

# 桌面端官方模型缓存的备选位置（macOS 桌面端可能落在 App Support）
MODELS_CACHE_CANDIDATES = [
    os.path.join(CODEX_DIR, "models_cache.json"),
    os.path.join(HOME, "Library/Application Support/Codex/models_cache.json"),
]

# shell 启动文件里的托管块标记
RC_BEGIN = "# >>> codex-switch-provider (managed) >>>"
RC_END = "# <<< codex-switch-provider (managed) <<<"
RC_CANDIDATES = [os.path.join(HOME, ".zshrc"),
                 os.path.join(HOME, ".bash_profile"),
                 os.path.join(HOME, ".bashrc")]

ENV_KEYS = ("CODEX_API_KEY", "OPENAI_API_KEY", "OPENAI_BASE_URL")

# 官方模型兜底清单（models_cache.json 不可用时使用，2026-09 校准）
OFFICIAL_FALLBACK = [
    {"id": "gpt-5.6-terra", "label": "GPT-5.6-Terra"},
    {"id": "gpt-5.6-luna", "label": "GPT-5.6-Luna"},
    {"id": "gpt-reserve", "label": "GPT-Reserve"},
    {"id": "gpt-5.5", "label": "GPT-5.5"},
    {"id": "gpt-5.4-mini", "label": "GPT-5.4-Mini"},
    {"id": "codex-auto-review", "label": "Codex Auto Review"},
]

# 探测结果里需要剔除的非对话模型
NON_CHAT_RE = re.compile(r"(?i)embedding|whisper|tts|speech|dall-e|moderation|rerank")
SECTION_RE = re.compile(r"^\s*\[.+\]\s*(#.*)?$")

# catalog 条目的兜底模板：当 models_cache.json / models.json 都不存在（如全新 Mac）时克隆。
# 字段集依据 codex-rs 模型条目 schema（缺 supported_reasoning_levels 等必填字段时，
# 桌面端报 "missing field `supported_reasoning_levels`"，整个 catalog 拒绝解析）。
# 可用内置 CLI 本地校验：codex debug models
FALLBACK_MODEL_TEMPLATE = {
    "slug": "",
    "display_name": "",
    "description": "",
    "default_reasoning_level": "medium",
    "supported_reasoning_levels": [
        {"effort": "low", "description": "Fast responses with lighter reasoning"},
        {"effort": "medium", "description": "Balances speed and reasoning depth for everyday tasks"},
        {"effort": "high", "description": "Greater reasoning depth for complex problems"},
        {"effort": "xhigh", "description": "Extra high reasoning depth for complex problems"},
        {"effort": "max", "description": "Maximum reasoning depth for the hardest problems"},
        {"effort": "ultra", "description": "Maximum reasoning with automatic task delegation"},
    ],
    "shell_type": "shell_command",
    "visibility": "list",
    "minimal_client_version": [0, 99, 0],
    "supported_in_api": True,
    "priority": 1,
    "upgrade": None,
    "base_instructions": "",
    "supports_reasoning_summaries": False,
    "support_verbosity": False,
    "default_verbosity": None,
    "apply_patch_tool_type": None,
    "truncation_policy": {"mode": "bytes", "limit": 100000},
    "supports_parallel_tool_calls": True,
    "supports_image_detail_original": False,
    "context_window": 272000,
    "experimental_supported_tools": [],
}

# ---------------------------------------------------------------- 输出着色

_TTY = sys.stdout.isatty()
def _c(code: str) -> str:
    return code if _TTY else ""

C_BLUE, C_GREEN, C_YELLOW, C_RED, C_CYAN, C_GRAY, C_RESET = (
    _c("\033[34m"), _c("\033[32m"), _c("\033[33m"),
    _c("\033[31m"), _c("\033[36m"), _c("\033[90m"), _c("\033[0m"),
)


def info(m):  print(f"{C_BLUE}[信息]{C_RESET} {m}")
def ok(m):    print(f"{C_GREEN}[成功]{C_RESET} {m}")
def warn(m):  print(f"{C_YELLOW}[警告]{C_RESET} {m}")
def err(m):   print(f"{C_RED}[错误]{C_RESET} {m}")


def interactive() -> bool:
    """计划任务 / 管道等非交互场景自动跳过菜单。"""
    try:
        return sys.stdin.isatty()
    except Exception:
        return False


# ---------------------------------------------------------------- 文件小工具

def read_text(path: str) -> str:
    """UTF-8 读取并剥离 BOM（旧版 PS 写入的文件可能带 BOM）。"""
    with open(path, "r", encoding="utf-8-sig", errors="replace") as f:
        return f.read()


def write_text(path: str, text: str):
    """UTF-8 无 BOM 写入（Codex 的 TOML/JSON 解析器不接受 BOM）。"""
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def read_json(path: str):
    try:
        return json.loads(read_text(path))
    except Exception:
        return None


def write_json(path: str, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    write_text(path, json.dumps(obj, ensure_ascii=False, indent=4) + "\n")


# ---------------------------------------------------------------- TOML 编辑

def toml_str(value: str) -> str:
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'


def config_lines():
    if os.path.exists(CODEX_CONFIG):
        return read_text(CODEX_CONFIG).splitlines()
    return []


def root_key(name: str):
    """读取 TOML 根区域（首个 [section] 之前）的字符串键值。"""
    for line in config_lines():
        if SECTION_RE.match(line):
            break
        m = re.match(r'^\s*' + re.escape(name) + r'\s*=\s*"([^"]*)"', line)
        if m:
            return m.group(1)
    return None


def set_section_values(lines, section, settings):
    """原位替换已有键，缺失键追加到该 section 末尾。

    settings: [(key, toml_value_string), ...]，保持顺序。
    section 为空字符串表示 TOML 根区域。
    """
    is_root = not section
    found = is_root
    start, end = 0, len(lines)

    if is_root:
        for i, line in enumerate(lines):
            if SECTION_RE.match(line):
                end = i
                break
    else:
        sec_re = re.compile(r'^\s*\[' + re.escape(section) + r'\]\s*(#.*)?$')
        for i, line in enumerate(lines):
            if sec_re.match(line):
                found = True
                start = i + 1
                end = len(lines)
                for j in range(start, len(lines)):
                    if SECTION_RE.match(lines[j]):
                        end = j
                        break
                break

    if not found:
        if lines and lines[-1].strip():
            lines.append("")
        lines.append(f"[{section}]")
        for k, v in settings:
            lines.append(f"{k} = {v}")
        return lines

    updated = set()
    for i in range(start, end):
        for k, v in settings:
            if re.match(r'^(\s*)' + re.escape(k) + r'\s*=.*$', lines[i]):
                indent = re.match(r'^(\s*)', lines[i]).group(1)
                lines[i] = f"{indent}{k} = {v}"
                updated.add(k)
                break

    offset = 0
    for k, v in settings:
        if k not in updated:
            lines.insert(end + offset, f"{k} = {v}")
            offset += 1

    # 根区域与紧随其后的 [section] 之间补一个空行，保持文件可读
    if is_root:
        pos = end + offset
        if 0 < pos < len(lines) and lines[pos - 1].strip() and SECTION_RE.match(lines[pos]):
            lines.insert(pos, "")
    return lines


def remove_root_keys(lines, keys):
    """删除根区域（首个 [section] 之前）的指定键。"""
    out, in_root = [], True
    for line in lines:
        if in_root and SECTION_RE.match(line):
            in_root = False
        drop = False
        if in_root:
            for k in keys:
                if re.match(r'^\s*' + re.escape(k) + r'\s*=', line):
                    drop = True
                    break
        if not drop:
            out.append(line)
    return out


def backup_config():
    if os.path.exists(CODEX_CONFIG):
        bak = f"{CODEX_CONFIG}.backup.switch_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
        shutil.copy2(CODEX_CONFIG, bak)
        info(f"已备份配置到：{bak}")


# ---------------------------------------------------------------- 环境变量（macOS 关键差异）

def _rc_files():
    """返回需要维护的 shell 启动文件；都不存在时默认创建 ~/.zshrc（macOS 默认 zsh）。"""
    existing = [f for f in RC_CANDIDATES if os.path.exists(f)]
    return existing or [os.path.join(HOME, ".zshrc")]


def _unquote(rhs: str) -> str:
    rhs = rhs.strip()
    try:
        parts = shlex.split(rhs)
        if parts:
            return parts[0]
    except ValueError:
        pass
    return rhs.strip("'\"")


def _parse_block(text: str):
    """解析托管块 -> (块前文本, [(key,value)...], 块后文本)；无块返回 None。"""
    pat = re.compile(re.escape(RC_BEGIN) + r"(.*?)" + re.escape(RC_END), re.S)
    m = pat.search(text)
    if not m:
        return None
    pairs = []
    for line in m.group(1).splitlines():
        mm = re.match(r'^\s*export\s+([A-Za-z_][A-Za-z0-9_]*)=(.*)$', line)
        if mm:
            pairs.append((mm.group(1), _unquote(mm.group(2))))
    return text[:m.start()], pairs, text[m.end():]


def read_rc_env():
    """读取托管块里当前声明的环境变量（用于 -S 展示与残留检查）。"""
    merged = {}
    for rc in _rc_files():
        if not os.path.exists(rc):
            continue
        parsed = _parse_block(read_text(rc))
        if parsed:
            for k, v in parsed[1]:
                merged[k] = v
    return merged


def apply_env(updates):
    """把 updates（key -> 值；None 表示删除该变量）落到 shell rc 托管块 + launchctl。

    macOS 没有 Windows 的「用户级环境变量」，等价物是 shell 启动文件（所有新终端）
    加上 launchctl（Dock/Finder 启动的 GUI 应用）。两者都同步，行为才贴近 Windows。
    """
    for rc in _rc_files():
        try:
            text = read_text(rc) if os.path.exists(rc) else ""
        except Exception as e:
            warn(f"读取 {rc} 失败：{e}")
            continue
        parsed = _parse_block(text)
        if parsed:
            before, pairs, after = parsed
        else:
            before, pairs, after = text, [], ""

        merged = dict(pairs)
        order = [k for k, _ in pairs]
        for k, v in updates.items():
            if v is None:
                merged.pop(k, None)
                if k in order:
                    order.remove(k)
            else:
                if k not in merged:
                    order.append(k)
                merged[k] = v

        if merged:
            body = "\n".join(f"export {k}={shlex.quote(merged[k])}" for k in order if k in merged)
            block = f"{RC_BEGIN}\n{body}\n{RC_END}"
            parts = []
            if before.strip():
                parts.append(before.rstrip("\n"))
            parts.append(block)
            if after.strip():
                parts.append(after.lstrip("\n").rstrip("\n"))
            new_text = "\n".join(parts) + "\n"
        else:
            # 全部变量都被清除：整块移除，避免留下空标记
            rest = before.rstrip("\n")
            if after.strip():
                rest = (rest + "\n" + after.lstrip("\n")).strip("\n")
            new_text = (rest + "\n") if rest.strip() else ""

        try:
            write_text(rc, new_text)
            info(f"已更新 {rc} 中的托管块")
        except Exception as e:
            warn(f"写入 {rc} 失败：{e}")

    # 同步到 launchd，供 GUI 启动的应用继承
    for k, v in updates.items():
        try:
            if v is None:
                subprocess.run(["launchctl", "unsetenv", k],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
            else:
                subprocess.run(["launchctl", "setenv", k, v],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
        except Exception:
            pass


def find_unmanaged_exports():
    """找出托管块之外手写的同名 export —— 它们会覆盖托管块（官方模式下会劫持请求）。"""
    hits = []
    keys_re = re.compile(r'^\s*(?:export\s+)?(' + "|".join(ENV_KEYS) + r')\s*=')
    block_re = re.compile(re.escape(RC_BEGIN) + r".*?" + re.escape(RC_END), re.S)
    for rc in _rc_files():
        if not os.path.exists(rc):
            continue
        text = read_text(rc)
        m = block_re.search(text)
        span = (m.start(), m.end()) if m else (-1, -1)
        offset = 0
        for idx, line in enumerate(text.splitlines(True), 1):
            in_block = span[0] <= offset < span[1]
            if not in_block:
                mm = keys_re.match(line)
                if mm:
                    hits.append((rc, idx, mm.group(1)))
            offset += len(line)
    return hits


def clean_unmanaged_exports(targets):
    """注释掉托管块之外手写的指定变量行（--clean-env 时使用）。"""
    changed = []
    keys_re = re.compile(r'^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=')
    block_re = re.compile(re.escape(RC_BEGIN) + r".*?" + re.escape(RC_END), re.S)
    for rc in _rc_files():
        if not os.path.exists(rc):
            continue
        text = read_text(rc)
        m = block_re.search(text)
        span = (m.start(), m.end()) if m else (-1, -1)
        out, hits, offset = [], 0, 0
        for line in text.splitlines(True):
            in_block = span[0] <= offset < span[1]
            mm = keys_re.match(line)
            if not in_block and mm and mm.group(1) in targets:
                out.append(f"# [codex-switch 已禁用] {line.lstrip()}")
                hits += 1
            else:
                out.append(line)
            offset += len(line)
        if hits:
            write_text(rc, "".join(out))
            changed.append((rc, hits))
    return changed


# ---------------------------------------------------------------- 供应商清单

def load_providers():
    if not os.path.exists(PROV_FILE):
        seed = {
            "official": {"name": "OpenAI官方", "base_url": "", "wire_api": "responses",
                         "default_model": "gpt-5.6-terra", "api_key": ""},
            "sharedchat": {"name": "sharedchat", "base_url": "https://new.sharedchat.cc/codex",
                           "wire_api": "responses", "default_model": "gpt-5.6-sol", "api_key": ""},
        }
        write_json(PROV_FILE, seed)
        info(f"已生成供应商清单：{PROV_FILE}")
    data = read_json(PROV_FILE)
    if not isinstance(data, dict):
        err(f"供应商清单格式非法（应为 JSON 对象）：{PROV_FILE}")
        sys.exit(1)
    if OFFICIAL_ID not in data:
        data[OFFICIAL_ID] = {"name": "OpenAI官方", "base_url": "", "wire_api": "responses",
                             "default_model": "gpt-5.6-terra", "api_key": ""}
        save_providers(data)
        info("已自动添加内置官方供应商条目：official（ChatGPT 登录模式）")
    return data


def save_providers(data):
    write_json(PROV_FILE, data)


def load_models_cache():
    for path in MODELS_CACHE_CANDIDATES:
        if os.path.exists(path):
            data = read_json(path)
            if data:
                return data, path
    return None, None


# ---------------------------------------------------------------- 模型探测

def _http_get_json(url, api_key, timeout=12):
    """优先用 curl：系统 curl 的 TLS 指纹能过 Cloudflare 类防护（urllib 常被 403）。"""
    if shutil.which("curl"):
        try:
            r = subprocess.run(
                ["curl", "-s", "-m", str(timeout), "-A", "codex-switch-script/1.0",
                 "-H", f"Authorization: Bearer {api_key}", url],
                capture_output=True, text=True, timeout=timeout + 5)
            raw = (r.stdout or "").strip()
            if r.returncode == 0 and raw[:1] in "{[":
                try:
                    return json.loads(raw)
                except Exception:
                    pass
        except Exception:
            pass
    # urllib 兜底
    try:
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {api_key}",
            "User-Agent": "codex-switch-script/1.0",
        })
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8", "replace"))
    except Exception:
        return None


def get_available_models(pid, p):
    """返回 {"models": [{"id","label"}], "source": "..."}；探测失败返回 None。"""
    if pid == OFFICIAL_ID:
        cache, cache_path = load_models_cache()
        if cache:
            models = [{"id": m.get("slug"), "label": m.get("display_name")}
                      for m in (cache.get("models") or []) if m.get("slug")]
            if models:
                return {"models": models,
                        "source": f"桌面端缓存 {cache_path}（fetched_at {cache.get('fetched_at')}，"
                                  f"过期数据仅供参考，可 m 手动输入）"}
        return {"models": OFFICIAL_FALLBACK,
                "source": "内置已知官方模型清单（models_cache.json 不可用）"}

    base = (p.get("base_url") or "").rstrip("/")
    if not base:
        return None
    api_key = p.get("api_key") or ""
    urls = [f"{base}/models"]
    if not base.endswith("/v1"):
        m = re.match(r'^(https?://[^/]+)', base)
        if m:
            urls.append(f"{m.group(1)}/v1/models")

    for url in urls:
        resp = _http_get_json(url, api_key)
        if resp is None:
            continue
        items = resp if isinstance(resp, list) else (resp.get("data") if isinstance(resp, dict) else None)
        if not items:
            continue
        models, seen = [], set()
        for it in items:
            mid = it if isinstance(it, str) else (it.get("id") if isinstance(it, dict) else None)
            if not mid or mid in seen or NON_CHAT_RE.search(mid):
                continue
            seen.add(mid)
            models.append({"id": mid, "label": None})
        if models:
            models.sort(key=lambda x: x["id"])
            return {"models": models, "source": url}
    return None


def show_model_menu(p, probe):
    """交互选择模型；返回 (model, cancelled)。"""
    print()
    info("可用模型来源：" + probe["source"])
    print(f"{C_CYAN}可用模型（共 {len(probe['models'])} 个）：{C_RESET}")
    for i, m in enumerate(probe["models"], 1):
        label = f"  ({m['label']})" if m.get("label") else ""
        print(f"{i:>4}. {m['id']}{label}")
    default = p.get("default_model") or "(未设置)"
    print(f"{C_GRAY}   d. 使用默认模型 {default}{C_RESET}")
    print(f"{C_GRAY}   m. 手动输入模型 ID{C_RESET}")
    print(f"{C_GRAY}   0. 取消切换{C_RESET}")
    while True:
        try:
            choice = input("请选择（编号 / d / m / 0）: ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            return "", True
        if choice == "0":
            return "", True
        if choice == "d":
            if p.get("default_model"):
                return p["default_model"], False
            warn("该供应商未设置默认模型，请改用编号或 m 手动输入")
            continue
        if choice == "m":
            try:
                mid = input("请输入模型 ID: ").strip()
            except (EOFError, KeyboardInterrupt):
                print()
                return "", True
            if mid:
                return mid, False
            warn("模型 ID 不能为空")
            continue
        if choice.isdigit():
            n = int(choice)
            if 1 <= n <= len(probe["models"]):
                return probe["models"][n - 1]["id"], False
        warn(f"无效输入：{choice}（请输入 1-{len(probe['models'])} 的编号，或 d / m / 0）")


# ---------------------------------------------------------------- 模型 catalog

def find_codex_cli():
    """定位可用的 codex CLI：PATH 优先，其次桌面端内置。"""
    exe = shutil.which("codex")
    if not exe:
        bundled = "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex"
        if os.path.exists(bundled):
            exe = bundled
    return exe


def get_builtin_catalog():
    """收割桌面端内置官方模型目录（best-effort，失败返回 {}）。

    用临时 CODEX_HOME 隔离用户配置，确保渲染的是纯内置目录（含完整
    supported_reasoning_levels / context_window / 指令模板）。slug 相同的
    relay 模型直接克隆官方条目，档位与官方一致。
    """
    exe = find_codex_cli()
    if not exe:
        return {}
    tmp = tempfile.mkdtemp(prefix="codex-switch-builtin-")
    try:
        os.makedirs(os.path.join(tmp, ".codex"), exist_ok=True)
        env = dict(os.environ, CODEX_HOME=os.path.join(tmp, ".codex"))
        r = subprocess.run([exe, "debug", "models"],
                           capture_output=True, text=True, timeout=60, env=env)
        out = (r.stdout or "").strip()
        if r.returncode != 0 or not out.startswith("{"):
            return {}
        data = json.loads(out)
        return {m["slug"]: m for m in (data.get("models") or []) if m.get("slug")}
    except Exception:
        return {}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def build_provider_catalog(pid, p, probe_models, target_model):
    """生成供应商专属 catalog（config.toml 根级 model_catalog_json 指向它）。

    桌面端模型列表/显示名由该文件决定，切换供应商必须同步重指，否则显示错乱。
    合并优先级：当前 catalog 条目 > models.json（保留 template）> 官方缓存条目 > 官方条目克隆。
    返回 catalog 路径（正斜杠）；无法构建返回 None。
    """
    cat_dir = os.path.join(CODEX_DIR, "model-providers")
    os.makedirs(cat_dir, exist_ok=True)
    cat_path = os.path.join(cat_dir, f"catalog-{pid}.json")

    prior = {}
    cur = root_key("model_catalog_json")
    if cur:
        old = os.path.expanduser(cur)
        if os.path.exists(old) and os.path.abspath(old) != os.path.abspath(cat_path):
            old_data = read_json(old) or {}
            for m in (old_data.get("models") or []):
                if m.get("slug"):
                    prior[m["slug"]] = m

    mj = read_json(os.path.join(CODEX_DIR, "models.json")) or {}
    for m in (mj.get("models") or []):
        slug = m.get("slug")
        if slug and slug not in prior:
            prior[slug] = m

    official = {}
    cache, _ = load_models_cache()
    if cache:
        for m in (cache.get("models") or []):
            if m.get("slug"):
                official[m["slug"]] = m

    slugs = set()
    for m in (probe_models or []):
        if m.get("id"):
            slugs.add(m["id"])
    if target_model:
        slugs.add(target_model)
    slugs.update(prior.keys())
    if not slugs:
        return None

    tmpl = next((m for m in official.values() if m.get("model_messages")), None)

    # 内置官方目录（codex debug models）：slug 相同的模型克隆官方完整条目，
    # 能继承真实的推理档位/上下文窗口/指令模板
    builtin = get_builtin_catalog()
    if builtin:
        n = sum(1 for s in slugs if s in builtin)
        if n:
            info(f"已从内置官方目录克隆 {n} 个同名模型的完整条目")

    entries = []
    for i, slug in enumerate(sorted(slugs), 1):
        if slug in prior:
            entry = copy.deepcopy(prior[slug])
        elif slug in official:
            entry = copy.deepcopy(official[slug])
        elif slug in builtin:
            entry = copy.deepcopy(builtin[slug])
        elif tmpl:
            entry = copy.deepcopy(tmpl)
            entry["slug"] = slug
            entry["display_name"] = slug
            entry["description"] = f"via {pid} relay"
        else:
            # 无任何官方模板可克隆：用 FALLBACK_MODEL_TEMPLATE 补全必填字段，
            # 否则桌面端整个 catalog 拒绝解析
            entry = copy.deepcopy(FALLBACK_MODEL_TEMPLATE)
            entry["slug"] = slug
            entry["display_name"] = slug
            entry["description"] = f"via {pid} relay"
        # 兜底模板以外的来源可能只带 3 档（旧 catalog/models.json），统一补齐到完整档位
        if entry.get("supported_reasoning_levels") and \
                {l.get("effort") for l in entry["supported_reasoning_levels"]} < \
                {l["effort"] for l in FALLBACK_MODEL_TEMPLATE["supported_reasoning_levels"]}:
            entry["supported_reasoning_levels"] = copy.deepcopy(
                FALLBACK_MODEL_TEMPLATE["supported_reasoning_levels"])
        entry["priority"] = i
        entry["visibility"] = "list"
        entries.append(entry)

    if not entries:
        return None

    write_text(cat_path, json.dumps({"models": entries}, ensure_ascii=False, indent=2) + "\n")
    return cat_path.replace("\\", "/")


def validate_catalog_with_cli():
    """用桌面端内置 codex CLI 的 `debug models` 校验 catalog 能否被解析（best-effort）。

    这一步能拦住"文件生成了但 schema 不对导致桌面端拒绝解析"的静默失败
    （如缺 supported_reasoning_levels 字段）。无 CLI 时跳过。
    返回 None=跳过，True=通过，str=错误摘要。
    """
    exe = find_codex_cli()
    if not exe:
        return None
    try:
        r = subprocess.run([exe, "debug", "models"],
                           capture_output=True, text=True, timeout=60)
    except Exception:
        return None
    if r.returncode != 0 or (r.stderr or "").strip().startswith("Error"):
        first = ((r.stderr or "").strip() or (r.stdout or "").strip()).splitlines()
        return first[0][:300] if first else "未知错误"
    return True


# ---------------------------------------------------------------- 动作：-L

def action_list():
    providers = load_providers()
    current = root_key("model_provider") or ""
    if not current or current == OPENAI_BUILTIN:
        current = OFFICIAL_ID
    print()
    print(f"{C_CYAN}已配置的 Codex 供应商（* 为当前生效）：{C_RESET}")
    for pid, p in providers.items():
        mark = "*" if pid == current else " "
        url = p.get("base_url") or "(内置 api.openai.com)"
        key = p.get("api_key") or ""
        key_mask = (key[:8] + "...") if key else "(ChatGPT登录/未设Key)"
        print(f"{mark} {pid:<14} {url}  协议={p.get('wire_api')}  "
              f"默认模型={p.get('default_model')}  Key={key_mask}")
    print()
    info(f"供应商清单：{PROV_FILE}")
    # 两份清单同时存在时优先级容易搞混，明确提示（脚本同目录 > ~/.codex/providers.json）
    alt = os.path.join(CODEX_DIR, "providers.json")
    if os.path.abspath(PROV_FILE) != os.path.abspath(alt) and os.path.exists(alt):
        warn(f"同时存在 {alt}，但本脚本优先读同目录的 {PROV_FILE}。"
             f"想用那份清单请删掉/改名同目录 providers.json")
    placeholder = [pid for pid, p in providers.items()
                   if "<base_url>" in str(p.get("base_url"))
                   or "<api_key>" in str(p.get("api_key"))]
    if placeholder:
        warn(f"清单里 {', '.join(placeholder)} 仍是脱敏占位值（<base_url>/<api_key>），"
             "切换前请填入真实地址与 Key，或改用 ~/.codex/providers.json")


# ---------------------------------------------------------------- 动作：-S

def action_show():
    print()
    info("当前 config.toml 关键配置：")
    cur_provider, cur_model = "", ""
    if os.path.exists(CODEX_CONFIG):
        for line in config_lines():
            m = re.match(r'^\s*model_provider\s*=\s*"([^"]+)"', line)
            if m:
                cur_provider = m.group(1)
            m = re.match(r'^\s*model\s*=\s*"([^"]+)"', line)
            if m:
                cur_model = m.group(1)
            if re.match(r'^\s*(model|model_provider|model_reasoning_effort|model_catalog_json)\s*=', line) \
                    or re.match(r'^\[model_providers\.', line):
                print(f"  {line}")
    else:
        warn(f"未找到 {CODEX_CONFIG}")

    if not cur_provider or cur_provider == OPENAI_BUILTIN:
        info("当前模式：OpenAI 官方（model_provider 未设/内置，认证优先走 auth.json ChatGPT 登录）")
    else:
        info(f"当前模式：自定义供应商 {cur_provider}")

    # catalog 诊断
    cat_key = root_key("model_catalog_json")
    if cat_key:
        cat_abs = os.path.expanduser(cat_key)
        if os.path.exists(cat_abs):
            data = read_json(cat_abs)
            if data is None:
                warn(f"catalog 文件解析失败：{cat_key}")
            else:
                slugs = [m.get("slug") for m in (data.get("models") or [])]
                info(f"模型 catalog：{cat_key}（{len(slugs)} 个模型）")
                if cur_model and cur_model not in slugs:
                    warn(f"当前模型 '{cur_model}' 不在 catalog 中：桌面端模型显示会不准。"
                         f"重跑 -P {cur_provider} 刷新 catalog")
        else:
            warn(f"model_catalog_json 指向的文件不存在：{cat_key}（桌面端会静默回退内置元数据，显示不准）")
    elif cur_provider and cur_provider != OPENAI_BUILTIN:
        warn(f"自定义供应商模式但未设 model_catalog_json：桌面端模型显示会不准。"
             f"重跑 -P {cur_provider} 生成 catalog")

    # auth.json 登录态
    auth_path = os.path.join(CODEX_DIR, "auth.json")
    if os.path.exists(auth_path):
        auth = read_json(auth_path)
        if auth is None:
            warn("auth.json 解析失败")
        else:
            info(f"auth.json: auth_mode={auth.get('auth_mode')}  "
                 f"ChatGPT登录有效={bool(auth.get('tokens'))}")
    else:
        warn("未找到 auth.json（官方 ChatGPT 登录模式需要；如需官方 API Key 模式请给 official 配 api_key）")

    # 环境变量：托管块声明值 + 当前进程实际值
    rc_env = read_rc_env()
    print()
    info(f"环境变量托管块（{'、'.join(os.path.basename(f) for f in _rc_files())}）：")
    for k in ENV_KEYS:
        v = rc_env.get(k)
        if v is None:
            print(f"  {k} = (未设置)")
        elif "KEY" in k:
            print(f"  {k} = {v[:8]}...")
        else:
            print(f"  {k} = {v}")

    for k in ENV_KEYS:
        live = os.environ.get(k)
        if live and rc_env.get(k) != live:
            shown = live[:8] + "..." if "KEY" in k else live
            info(f"当前进程 {k} = {shown}（与托管块不同：可能是旧终端 session，重开终端即可）")

    # 劫持检查：官方模式下 OPENAI_BASE_URL 会劫持官方请求
    official_mode = (not cur_provider or cur_provider == OPENAI_BUILTIN)
    base_url = rc_env.get("OPENAI_BASE_URL") or os.environ.get("OPENAI_BASE_URL")
    if official_mode and base_url:
        warn(f"官方模式下检测到 OPENAI_BASE_URL={base_url}，会劫持官方请求！执行 -P official 可自动清理")

    # 托管块之外的手写 export 会覆盖托管块
    hits = find_unmanaged_exports()
    if hits:
        print()
        for rc, line_no, key in hits:
            warn(f"{rc}:{line_no} 手写的 {key} 不在托管块内，会覆盖本工具设置的值")
        warn("确认无用后可加 --clean-env -P official 自动注释掉这些行")
    print()


# ---------------------------------------------------------------- 动作：-Pr

def action_probe(target):
    if target == OPENAI_BUILTIN:
        info(f"已映射：{OPENAI_BUILTIN} -> 内置官方供应商 {OFFICIAL_ID}")
        target = OFFICIAL_ID
    providers = load_providers()
    p = providers.get(target)
    if not p:
        err(f"供应商 '{target}' 不存在。用 -L 查看已有供应商。")
        return
    info("正在探测可用模型 ...")
    res = get_available_models(target, p)
    if not res or not res["models"]:
        err("探测失败：该供应商不支持模型列表接口（或网络/鉴权异常）。切换时可 m 手动输入模型 ID 兜底。")
        return
    ok(f"探测到 {len(res['models'])} 个模型（{res['source']}）")
    for i, m in enumerate(res["models"], 1):
        label = f"  ({m['label']})" if m.get("label") else ""
        print(f"{i:>4}. {m['id']}{label}")
    print()
    info(f"执行 -P {target} 切换时会出现选择菜单（加 --auto 跳过菜单直接用默认模型）")


# ---------------------------------------------------------------- 动作：-A / -R

def action_add(pid, base_url, api_key, name, model, wire_api):
    if not base_url or not api_key:
        err("添加供应商需要同时提供 --base-url 和 --api-key")
        return
    providers = load_providers()
    if pid in providers:
        warn(f"供应商 '{pid}' 已存在，将覆盖")
    providers[pid] = {
        "name": name or pid,
        "base_url": base_url.rstrip("/"),
        "wire_api": wire_api or "responses",
        "default_model": model or "",
        "api_key": api_key,
    }
    save_providers(providers)
    ok(f"已保存供应商：{pid} -> {base_url}")
    info(f"执行 ./switch-codex-provider.py -P {pid} 即可切换")


def action_remove(pid):
    if pid in (OFFICIAL_ID, OPENAI_BUILTIN):
        err("内置官方供应商不可删除")
        return
    providers = load_providers()
    if pid not in providers:
        err(f"供应商 '{pid}' 不存在")
        return
    del providers[pid]
    save_providers(providers)
    ok(f"已删除供应商：{pid}")


# ---------------------------------------------------------------- 动作：-P

def action_switch(pid, model, auto, clean_env=False):
    if pid == OPENAI_BUILTIN:
        info(f"已映射：{OPENAI_BUILTIN} -> 内置官方供应商 {OFFICIAL_ID}")
        pid = OFFICIAL_ID
    providers = load_providers()
    p = providers.get(pid)
    if not p:
        err(f"供应商 '{pid}' 不存在。用 -L 查看已有供应商，或用 -A 添加。")
        return
    is_official = (pid == OFFICIAL_ID)

    # 1) 确定目标模型：-M 显式指定 > 交互选择 > 默认模型
    #    探测同时为 catalog 收集清单：任何切换都执行；失败不阻断切换
    target_model = model
    probe = get_available_models(pid, p)
    if probe and probe["models"]:
        ok(f"探测到 {len(probe['models'])} 个可用模型")
    else:
        probe = None
        warn("探测失败（接口不支持 /models 或网络/鉴权异常），不影响切换，仅模型 catalog 清单变少")

    if not target_model and not auto:
        if probe:
            if interactive():
                target_model, cancelled = show_model_menu(p, probe)
                if cancelled:
                    info("已取消切换")
                    return
            else:
                target_model = p.get("default_model")
                if target_model:
                    info(f"非交互环境：跳过菜单，使用默认模型 {target_model}")
        elif interactive():
            warn("自动探测失败（接口不支持 /models 或网络/鉴权异常），可手动输入模型 ID")
            hint = f"（直接回车使用默认模型 {p['default_model']}）" if p.get("default_model") else ""
            try:
                mid = input(f"请输入模型 ID{hint}: ").strip()
            except (EOFError, KeyboardInterrupt):
                mid = ""
                print()
            if mid:
                target_model = mid
            elif p.get("default_model"):
                target_model = p["default_model"]
                info(f"未输入，使用默认模型：{target_model}")
        else:
            warn("非交互环境且探测失败，回退使用默认模型")

    if not target_model:
        target_model = p.get("default_model")
    if not target_model:
        err("未指定模型：请用 -M 传入，或在探测菜单中选择/手动输入，"
            "或在 providers.json 中为该供应商设置 default_model")
        return

    # 2) 交互选定的模型回写为该供应商默认模型（-M 显式指定视为临时覆盖，不回写）
    if not model and (p.get("default_model") or "") != target_model:
        p["default_model"] = target_model
        save_providers(providers)
        ok(f"默认模型已更新：{pid}.default_model = {target_model}（写入 {PROV_FILE}）")

    backup_config()
    lines = config_lines()

    if is_official:
        # ===== 官方模式：删 model_provider + model_catalog_json 根键，恢复内置 provider 与内置模型目录
        lines = remove_root_keys(lines, ["model_provider", "model_catalog_json"])

        # 订阅登录模式下必须删掉残留的 preferred_auth_method / forced_login_method，
        # 它们会强制 Codex 只走 API Key 登录、禁用 ChatGPT 账号（订阅）登录
        if not p.get("api_key"):
            removed = [k for k in ("preferred_auth_method", "forced_login_method")
                       if any(re.match(r'^\s*' + re.escape(k) + r'\s*=', ln) for ln in lines)]
            if removed:
                lines = remove_root_keys(lines, removed)
                ok("已删除遗留认证根键 %s（该键强制 API Key 登录、禁用 ChatGPT 账号登录）"
                   % " / ".join(removed))

        lines = set_section_values(lines, "", [("model", toml_str(target_model))])
        write_text(CODEX_CONFIG, "\n".join(lines) + "\n")
        ok(f"config.toml 已更新：model_provider=(内置官方), model={target_model}")

        # 环境变量：OPENAI_BASE_URL 必清（否则劫持官方请求）；Key 按模式处理
        if p.get("api_key"):
            apply_env({"OPENAI_BASE_URL": None,
                       "CODEX_API_KEY": p["api_key"],
                       "OPENAI_API_KEY": p["api_key"]})
            ok("官方 API Key 模式：CODEX_API_KEY / OPENAI_API_KEY 已设置，OPENAI_BASE_URL 已清理")
        else:
            apply_env({k: None for k in ENV_KEYS})
            ok("ChatGPT 登录模式：已清理残留 Key/BaseUrl 环境变量（认证走 auth.json）")
            if clean_env:
                for rc, n in clean_unmanaged_exports(set(ENV_KEYS)):
                    ok(f"已注释掉 {rc} 中 {n} 行手写环境变量")

        if not os.path.exists(os.path.join(CODEX_DIR, "auth.json")) and not p.get("api_key"):
            warn("未检测到 auth.json 且未配置 api_key：请先运行 codex login 完成 ChatGPT 登录，"
                 "或给 official 配置 api_key")
    else:
        # ===== relay 模式：写供应商 section + 根选择器 + catalog
        lines = set_section_values(lines, f"model_providers.{pid}", [
            ("name", toml_str(p.get("name") or pid)),
            ("base_url", toml_str(p.get("base_url") or "")),
            ("wire_api", toml_str(p.get("wire_api") or "responses")),
            ("env_key", toml_str("CODEX_API_KEY")),
            ("supports_websockets", "false"),
        ])

        root_settings = [("model_provider", toml_str(pid)),
                         ("model", toml_str(target_model))]
        cat_path = build_provider_catalog(pid, p, probe["models"] if probe else None, target_model)
        if cat_path:
            root_settings.append(("model_catalog_json", toml_str(cat_path)))
            ok(f"模型 catalog 已更新：{cat_path}")
        else:
            warn("模型 catalog 生成失败，保留原 model_catalog_json（桌面端模型显示可能不准）")

        lines = set_section_values(lines, "", root_settings)
        write_text(CODEX_CONFIG, "\n".join(lines) + "\n")
        ok(f"config.toml 已更新：model_provider={pid}, model={target_model}")

        # 本地校验 catalog 能否被 Codex 解析（拦住 schema 不对的静默失败）
        vres = validate_catalog_with_cli()
        if vres is True:
            ok("catalog 已通过内置 CLI（codex debug models）解析校验")
        elif isinstance(vres, str):
            err(f"catalog 校验失败，桌面端将拒绝解析：{vres}")
            warn(f"请检查 {cat_path} 的字段结构")

        if p.get("api_key"):
            apply_env({"CODEX_API_KEY": p["api_key"],
                       "OPENAI_API_KEY": p["api_key"],
                       "OPENAI_BASE_URL": p.get("base_url") or ""})
            ok("环境变量 CODEX_API_KEY / OPENAI_API_KEY / OPENAI_BASE_URL 已写入 "
               f"{'、'.join(os.path.basename(f) for f in _rc_files())} + launchctl")
        else:
            warn(f"该供应商未配置 api_key，请编辑 {PROV_FILE} 补充，否则认证会失败")

    print()
    dest = "OpenAI 官方" if is_official else f"{pid} ({p.get('base_url')})"
    ok(f"切换完成：{dest} 模型={target_model}")
    warn("新终端才会继承新环境变量：当前窗口请执行 source ~/.zshrc 或直接开新标签")
    warn("ChatGPT 桌面端需完全退出（⌘Q）再重开，否则会回写旧的 model 键")


# ---------------------------------------------------------------- 动作：-F

def action_find(keyword):
    db = os.path.join(CODEX_DIR, "state_5.sqlite")
    if not os.path.exists(db):
        err(f"未找到 {db}")
        return
    sql = """
        SELECT id, model_provider, model, archived, cwd,
               substr(COALESCE(first_user_message,''),1,60),
               substr(COALESCE(title,''),1,40),
               datetime(COALESCE(recency_at,0), 'unixepoch', 'localtime')
        FROM threads
        WHERE first_user_message LIKE ? OR title LIKE ? OR preview LIKE ? OR cwd LIKE ?
        ORDER BY recency_at DESC LIMIT 20
    """
    kw = f"%{keyword}%"
    try:
        con = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=10)
        try:
            rows = con.execute(sql, (kw, kw, kw, kw)).fetchall()
        finally:
            con.close()
    except sqlite3.Error as e:
        err(f"查询失败：{e}")
        return
    if not rows:
        warn(f"没有匹配关键词的会话：{keyword}")
        return
    for r in rows:
        print(f"ID: {r[0]}")
        print(f"   provider: {r[1]} | model: {r[2]} | archived: {r[3]} | 最近活跃: {r[7]}")
        print(f"   cwd: {r[4]}")
        print(f"   首条消息: {str(r[5]).replace(chr(10), ' ')}")
    print()
    info("用 -Se <ID> 改写该会话绑定的供应商")


# ---------------------------------------------------------------- 动作：-Se

def action_session(session, provider, model):
    target = provider
    if not target:
        target = root_key("model_provider")
    if not target:
        err(f"无法确定目标供应商（未找到 {CODEX_CONFIG} 中的 model_provider），请用 -P 指定")
        return
    if target == OFFICIAL_ID:
        target = OPENAI_BUILTIN
        info(f"会话改写目标映射：official -> {OPENAI_BUILTIN}")

    # 解析会话文件：直接路径 / session id / 文件名关键字
    files = []
    if os.path.exists(session):
        files = [os.path.abspath(session)]
    else:
        for root in (os.path.join(CODEX_DIR, "sessions"),
                     os.path.join(CODEX_DIR, "archived_sessions")):
            if not os.path.isdir(root):
                continue
            for dirpath, _, names in os.walk(root):
                for n in names:
                    if n.endswith(".jsonl") and session in n:
                        files.append(os.path.join(dirpath, n))
    if not files:
        err(f"未找到匹配的会话文件：{session}")
        return
    if len(files) > 1:
        warn(f"匹配到 {len(files)} 个文件，请用更精确的 id 或完整路径：")
        for f in files:
            print(f"  {f}")
        return

    path = files[0]
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    bak = f"{path}.bak-switch-{stamp}"
    shutil.copy2(path, bak)

    old_ids = set()
    out_lines = []
    for line in read_text(path).splitlines():
        new_line = line
        # 替换 model_provider / model_provider_id 的值（会话 turn_context 快照）
        for key in ("model_provider_id", "model_provider"):
            pat = '("' + key + '")(\\s*:\\s*")([^"]*)(")'
            m = re.search(pat, new_line)
            if m:
                old_ids.add(m.group(3))
                new_line = re.sub(pat, lambda mm: mm.group(1) + mm.group(2) + target + mm.group(4),
                                  new_line)
        if model:
            new_line = re.sub(r'("model")(\s*:\s*")[^"]*(")',
                              lambda mm: mm.group(1) + mm.group(2) + model + mm.group(3),
                              new_line)
        out_lines.append(new_line)

    if not old_ids or (old_ids == {target}):
        warn("会话 rollout 中没有需要改写的供应商记录（或已是指定供应商）")
        os.remove(bak)
    else:
        write_text(path, "\n".join(out_lines) + "\n")
        ok(f"会话 rollout 已改写：{', '.join(sorted(old_ids))} -> {target}"
           + (f"，模型 -> {model}" if model else ""))
        info(f"文件：{path}")
        info(f"已备份：{bak}")

    # 同步桌面端会话注册表：恢复会话时读的是这里的 model_provider / model 列
    db = os.path.join(CODEX_DIR, "state_5.sqlite")
    if not os.path.exists(db):
        return
    leaf = os.path.basename(path)
    m = re.search(r'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.jsonl$', leaf)
    thread_id = m.group(1) if m else ""

    sets, args = ["model_provider = ?"], [target]
    if model:
        sets.append("model = ?")
        args.append(model)
    args += [thread_id, path]
    try:
        con = sqlite3.connect(db, timeout=10)
        try:
            cur = con.cursor()
            cur.execute("UPDATE threads SET " + ", ".join(sets)
                        + " WHERE id = ? OR rollout_path = ?", args)
            con.commit()
            n = cur.rowcount
        finally:
            con.close()
        if n == 0:
            warn("threads 表中未找到该会话记录（可能从未在桌面端打开过）")
        else:
            ok(f"桌面端会话注册表已同步：threads.model_provider -> {target}（{n} 条）")
    except sqlite3.OperationalError as e:
        err(f"sqlite 同步失败：{e}")
        info("排查提示：若报 database is locked，请 ⌘Q 彻底退出 ChatGPT 桌面端后重试")

    warn("改写后请完全退出 ChatGPT 桌面端（⌘Q）再重开，恢复该会话即走新供应商")


# ---------------------------------------------------------------- 帮助

HELP = """
Codex 供应商切换工具（macOS）

  -L, --list                列出所有供应商（含内置官方 official）
  -S, --show                查看当前配置（模式判定 + 环境变量残留警告）
  -Pr, --probe <id>         只探测列出该供应商可用模型，不切换
  -P, --provider <id>       切换供应商：自动探测模型并交互选择（选中即回写默认模型）
                            加 -M 显式指定模型；加 --auto 跳过菜单直接用默认模型
  -P official               切回 OpenAI 官方（ChatGPT 登录走 auth.json，自动清理劫持环境变量与模型 catalog）
  -A, --add <id>            添加供应商（需 --base-url --api-key，可选 -M --name --wire-api）
  -R, --remove <id>         删除供应商（official 不可删）
  -Se, --session <id|路径>  改写指定会话钉死的供应商为当前供应商（可选 -M 同步改模型）
  -F, --find <关键词>       按关键词搜索会话，拿到 session id 后配合 -Se 使用
      --clean-env           切回官方时，把 shell rc 中手写的同名 export 行一并注释掉
  -h, --help                显示本帮助

模型 catalog：relay 切换时自动生成 ~/.codex/model-providers/catalog-<id>.json 并重指
  model_catalog_json，桌面端模型列表/显示名以此为准；-S 可诊断 catalog 与当前模型是否匹配
环境变量（macOS）：写入 shell rc 托管块 + launchctl，新终端/新启动的桌面端才会生效
"""


def main():
    ap = argparse.ArgumentParser(add_help=False, description="Codex 供应商切换工具（macOS）")
    ap.add_argument("-h", "--help", action="store_true")
    ap.add_argument("-L", "--list", action="store_true")
    ap.add_argument("-S", "--show", action="store_true")
    ap.add_argument("-P", "--provider")
    ap.add_argument("-M", "--model")
    ap.add_argument("-A", "--add")
    ap.add_argument("--base-url", dest="base_url")
    ap.add_argument("-K", "--api-key", dest="api_key")
    ap.add_argument("--name")
    ap.add_argument("--wire-api", dest="wire_api", default="responses")
    ap.add_argument("-R", "--remove")
    ap.add_argument("-Se", "--session")
    ap.add_argument("-F", "--find")
    ap.add_argument("-Pr", "--probe")
    ap.add_argument("--auto", action="store_true")
    ap.add_argument("--clean-env", dest="clean_env", action="store_true")

    if len(sys.argv) == 1:
        print(HELP)
        return
    try:
        a = ap.parse_args()
    except SystemExit:
        return

    if a.help:
        print(HELP)
    elif a.list:
        action_list()
    elif a.show:
        action_show()
    elif a.probe:
        action_probe(a.probe)
    elif a.remove:
        action_remove(a.remove)
    elif a.add:
        action_add(a.add, a.base_url, a.api_key, a.name, a.model, a.wire_api)
    elif a.find:
        action_find(a.find)
    elif a.session:
        action_session(a.session, a.provider, a.model)
    elif a.provider:
        action_switch(a.provider, a.model, a.auto, a.clean_env)
    else:
        print(HELP)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print()
        sys.exit(130)
