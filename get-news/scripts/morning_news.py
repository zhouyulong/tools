#!/usr/bin/env python3
"""
晨间新闻聚合脚本
获取世界局势、经济政策、AI技术等高实时性信息
"""

import argparse
import json
import os
import re
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Tuple
from urllib.parse import urljoin

import feedparser
import pytz
import requests
from dateutil import parser as date_parser
from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel
from rich.table import Table

console = Console()

# ============== 配置 ==============

RSS_SOURCES = {
    "world": {
        "BBC World": "https://feeds.bbci.co.uk/news/world/rss.xml",
        "NYT World": "https://rss.nytimes.com/services/xml/rss/nyt/World.xml",
        "UN News": "https://news.un.org/feed/subscribe/en/news/all/rss.xml",
        "The Guardian World": "https://www.theguardian.com/world/rss",
    },
    "middle_east": {
        "Al Jazeera ME": "https://www.aljazeera.com/xml/rss/all.xml",
        "BBC Middle East": "https://feeds.bbci.co.uk/news/world/middle_east/rss.xml",
    },
    "economy_cn": {
        "财新": "https://weekly.caixin.com/rss.xml",
        "财新首页": "https://www.caixin.com/rss.xml",
    },
    "economy_us": {
        "Fed News": "https://www.federalreserve.gov/feeds/press_all.xml",
        "US Treasury": "https://home.treasury.gov/rss.xml",
        "CNBC": "https://www.cnbc.com/id/100003114/device/rss/rss.html",
        "NYT Business": "https://rss.nytimes.com/services/xml/rss/nyt/Business.xml",
    },
    "economy_eu": {
        "ECB News": "https://www.ecb.europa.eu/rss/press.html",
        "Financial Times": "https://www.ft.com/?format=rss",
    },
    "ai": {
        "arXiv AI": "https://export.arxiv.org/rss/cs.AI",
        "arXiv CL": "https://export.arxiv.org/rss/cs.CL",
        "arXiv LG": "https://export.arxiv.org/rss/cs.LG",
        "MIT Tech Review": "https://www.technologyreview.com/feed/",
    },
}

# API 端点
APIS = {
    "hackernews": "https://hacker-news.firebaseio.com/v0",
    "semantic_scholar": "https://api.semanticscholar.org/graph/v1",
}

# AI 摘要配置
AI_SUMMARY_CONFIG = {
    "enabled": os.environ.get("ENABLE_AI_SUMMARY", "false").lower() == "true",
    "api_key": os.environ.get("NEW_ANTHROPIC_API_KEY") or os.environ.get("NEW_OPENAI_API_KEY"),
    "model": os.environ.get("AI_MODEL", "claude-sonnet-4-6"),
    "max_items": int(os.environ.get("AI_MAX_ITEMS", "30")),  # 限制AI处理数量避免费用过高
}

# 可信度评分
CREDIBILITY = {
    "Reuters": 5,
    "BBC": 5,
    "NYT": 5,
    "UN News": 5,
    "Federal Reserve": 5,
    "ECB": 5,
    "Al Jazeera": 4,
    "The Guardian": 4,
    "Financial Times": 5,
    "财新": 4,
    "CNBC": 4,
    "arXiv": 5,
    "MIT Tech Review": 5,
    "Hacker News": 4,
    "GitHub": 4,
    "Semantic Scholar": 5,
}

# AI/Agent 相关关键词
AI_KEYWORDS = [
    "AI", "artificial intelligence", "machine learning", "deep learning",
    "agent", "llm", "large language model", "GPT", "Claude", "OpenAI",
    "memory", "RAG", "retrieval", "fine-tuning", "transformer", "skills"
    "神经网络", "深度学习", "大模型", "智能体", "记忆",
]


# ============== 摘要生成器 ==============

class NewsSummarizer:
    """新闻摘要生成器 - 支持本地规则和AI API两种模式"""

    def __init__(self):
        self.enabled = AI_SUMMARY_CONFIG["enabled"]
        self.api_key = AI_SUMMARY_CONFIG["api_key"]
        self.model = AI_SUMMARY_CONFIG["model"]
        self.max_items = AI_SUMMARY_CONFIG["max_items"]
        self.cache = {}  # 简单缓存避免重复处理

    def generate_summary(self, title: str, summary: str, source: str) -> Dict[str, str]:
        """生成新闻摘要和中英文对照

        Returns:
            {
                "brief": "一句话中文摘要",
                "en_title": "英文标题（翻译或原文）",
                "cn_title": "中文标题（翻译或原文）",
                "key_points": ["要点1", "要点2"],
                "context": "背景说明"
            }
        """
        cache_key = f"{title}:{summary[:50]}"
        if cache_key in self.cache:
            return self.cache[cache_key]

        # 如果启用AI且有API key，使用AI生成
        if self.enabled and self.api_key:
            result = self._generate_with_ai(title, summary, source)
        else:
            # 使用本地规则生成
            result = self._generate_local(title, summary, source)

        self.cache[cache_key] = result
        return result

    def _generate_local(self, title: str, summary: str, source: str) -> Dict[str, str]:
        """基于规则的本地摘要生成（无需API）"""
        # 检测语言
        is_chinese = any('\u4e00' <= c <= '\u9fff' for c in title)

        # 提取关键实体和动作
        key_info = self._extract_key_info(title, summary)

        if is_chinese:
            cn_title = title
            en_title = self._simple_translate_to_en(title)
        else:
            en_title = title
            cn_title = self._simple_translate_to_cn(title, summary)

        # 生成一句话摘要
        brief = self._generate_brief(cn_title, key_info, source)

        # 提取要点
        key_points = self._extract_key_points(summary, is_chinese)

        # 生成背景说明
        context = self._generate_context(source, key_info)

        return {
            "brief": brief,
            "en_title": en_title,
            "cn_title": cn_title,
            "key_points": key_points,
            "context": context
        }

    def _extract_key_info(self, title: str, summary: str) -> Dict:
        """提取关键信息"""
        text = f"{title} {summary}".lower()

        # 检测实体类型
        entities = {
            "country": [],
            "organization": [],
            "person": [],
            "topic": []
        }

        # 国家/地区关键词
        country_keywords = {
            "中国": "China", "美国": "USA", "us": "USA", "usa": "USA", "america": "USA",
            "俄罗斯": "Russia", "russia": "Russia",
            "伊朗": "Iran", "iran": "Iran",
            "以色列": "Israel", "israel": "Israel",
            "乌克兰": "Ukraine", "ukraine": "Ukraine",
            "欧洲": "Europe", "europe": "Europe",
            "日本": "Japan", "japan": "Japan",
            "英国": "UK", "uk": "UK", "britain": "UK",
            "德国": "Germany", "germany": "Germany",
            "法国": "France", "france": "France"
        }

        for keyword, en_name in country_keywords.items():
            if keyword.lower() in text:
                entities["country"].append(en_name)

        # 组织关键词
        org_keywords = ["fed", "ecb", "un", "nato", "eu", "opec", "who", "wto", "imf"]
        for org in org_keywords:
            if org in text:
                entities["organization"].append(org.upper())

        # 主题关键词
        topic_keywords = {
            "war": "军事冲突", "conflict": "冲突", "strike": "袭击",
            "economy": "经济", "trade": "贸易", "tariff": "关税",
            "ai": "AI", "artificial intelligence": "人工智能",
            "policy": "政策", "election": "选举", "vote": "投票"
        }

        for keyword, cn_name in topic_keywords.items():
            if keyword in text:
                entities["topic"].append(cn_name)

        return entities

    def _simple_translate_to_cn(self, title: str, summary: str) -> str:
        """智能提取关键实体并生成中文标题 - 增强版"""
        text_lower = f"{title} {summary}".lower()
        
        # 扩展的实体映射字典
        translations = {
            # 人物
            'trump': '特朗普', 'biden': '拜登', 'vance': '万斯', 'putin': '普京',
            'netanyahu': '内塔尼亚胡', 'zelensky': '泽连斯基', 'orban': '欧尔班',
            'melania': '梅拉尼娅', 'keir starmer': '基尔·斯塔默', 'min aung hlaing': '敏昂莱',
            'tyson fury': '泰森·富里', 'makhmudov': '马克穆多夫',
            
            # 国家/地区
            'iran': '伊朗', 'israel': '以色列', 'palestine': '巴勒斯坦', 'gaza': '加沙',
            'ukraine': '乌克兰', 'russia': '俄罗斯', 'china': '中国',
            'us': '美国', 'usa': '美国', 'america': '美国', 'uk': '英国',
            'pakistan': '巴基斯坦', 'syria': '叙利亚', 'lebanon': '黎巴嫩',
            'senegal': '塞内加尔', 'indonesia': '印度尼西亚', 'djibouti': '吉布提',
            'myanmar': '缅甸', 'kuwait': '科威特', 'venezuela': '委内瑞拉',
            'gulf': '海湾地区', 'middle east': '中东', 'europe': '欧洲',
            
            # 组织
            'nato': '北约', 'eu': '欧盟', 'un': '联合国', 'fed': '美联储',
            'ecb': '欧洲央行', 'hamas': '哈马斯', 'hezbollah': '真主党',
            'white house': '白宫',
            
            # 动作/事件
            'war': '战争', 'attack': '袭击', 'strike': '打击', 'ceasefire': '停火',
            'sanction': '制裁', 'tariff': '关税', 'trade': '贸易', 'deal': '协议',
            'agrees': '同意', 'rejects': '拒绝', 'announces': '宣布', 'warns': '警告',
            'threatens': '威胁', 'confirms': '确认', 'labels': '称', 'calls': '呼吁',
            'discovers': '发现', 'votes': '投票', 'seeks': '寻求', 'sworn in': '宣誓就职',
            'insists': '坚持', 'denies': '否认', 'urges': '敦促', 'blames': '指责',
            'describes': '描述', 'pushes back': '击退',
            
            # 主题
            'economy': '经济', 'oil': '石油', 'energy': '能源', 'prices': '价格',
            'submarine': '潜艇', 'cables': '电缆', 'pipelines': '管道',
            'refugees': '难民', 'election': '选举', 'presidential': '总统',
            'orthodox easter': '东正教复活节', 'journalist': '记者',
            'prosthetic eye': '假眼', 'prison': '监狱', 'drone': '无人机',
            'shipping': '航运', 'strait of hormuz': '霍尔木兹海峡',
            'prediction markets': '预测市场', 'bets': '赌注',
            'artificial intelligence': '人工智能', 'ai': 'AI', 'llm': '大语言模型',
            'machine learning': '机器学习',
        }
        
        # 提取匹配的关键词
        found_entities = []
        for en, cn in translations.items():
            # 使用单词边界匹配
            pattern = r'' + re.escape(en) + r''
            if re.search(pattern, text_lower):
                found_entities.append((en, cn))
        
        # 根据匹配到的实体构建中文标题
        if found_entities:
            # 最多取3个关键词
            selected = found_entities[:3]
            cn_parts = [cn for _, cn in selected]
            return ' '.join(cn_parts)
        
        # 如果无法提取，返回简化版原文
        return title[:50] + "..." if len(title) > 50 else title

    def _simple_translate_to_en(self, title: str) -> str:
        """简单的中文到英文翻译"""
        # 如果是纯中文，标记为需要翻译
        if all('\u4e00' <= c <= '\u9fff' or c in '，。！？：""''（）【】 ' for c in title):
            return f"[CN] {title}"
        return title

    def _generate_brief(self, cn_title: str, key_info: Dict, source: str) -> str:
        """生成一句话中文摘要"""
        # 基于关键词生成简要说明
        topics = key_info.get("topic", [])
        countries = key_info.get("country", [])

        if "军事冲突" in topics or "冲突" in topics:
            if countries:
                return f"涉及{', '.join(countries[:2])}的军事冲突最新进展"
            return "地区军事冲突局势更新"

        if "经济" in topics or "贸易" in topics:
            if "美国" in str(countries) or "USA" in countries:
                return "美国经济政策动态"
            return "全球经济形势更新"

        if "AI" in topics or "人工智能" in topics:
            return "人工智能领域技术进展"

        if "政策" in topics:
            return "重要政策发布或调整"

        if source in ["Fed News", "ECB News", "US Treasury"]:
            return "央行或财政部政策动态"

        if "arXiv" in source:
            return "学术研究最新成果"

        return "国际时事动态"

    def _extract_key_points(self, summary: str, is_chinese: bool) -> List[str]:
        """提取关键要点"""
        points = []

        # 清理HTML
        clean_summary = re.sub(r'<[^>]+>', '', summary)

        # 尝试提取句子
        sentences = re.split(r'[.!?。！？]+', clean_summary)

        for sent in sentences[:2]:  # 取前两句
            sent = sent.strip()
            if len(sent) > 10:
                if is_chinese:
                    points.append(sent[:80] + "..." if len(sent) > 80 else sent)
                else:
                    # 简单翻译关键片段
                    points.append(self._simple_translate_to_cn(sent, ""))

        if not points:
            points = ["详情请点击查看原文"]

        return points[:2]

    def _generate_context(self, source: str, key_info: Dict) -> str:
        """生成背景说明"""
        countries = key_info.get("country", [])
        orgs = key_info.get("organization", [])

        context_parts = []

        if source in ["BBC World", "BBC Middle East"]:
            context_parts.append("BBC国际新闻")
        elif source in ["NYT World", "NYT Business"]:
            context_parts.append("纽约时报")
        elif "Al Jazeera" in source:
            context_parts.append("半岛电视台")
        elif "UN News" in source:
            context_parts.append("联合国新闻")
        elif source == "Fed News":
            context_parts.append("美联储官方发布")
        elif source == "ECB News":
            context_parts.append("欧洲央行")
        elif "arXiv" in source:
            context_parts.append("学术预印本平台")
        else:
            context_parts.append(source)

        if countries:
            context_parts.append(f"涉及地区: {', '.join(countries[:3])}")

        return " | ".join(context_parts)

    def _generate_with_ai(self, title: str, summary: str, source: str) -> Dict[str, str]:
        """使用AI API生成高质量摘要和翻译"""
        prompt = f"""请分析以下新闻并提供结构化信息：

标题: {title}
摘要: {summary}
来源: {source}

请以JSON格式返回：
{{
    "brief": "一句话中文摘要（30字以内）",
    "en_title": "英文标题（如果是中文则翻译为英文，英文则保留）",
    "cn_title": "中文标题（如果是英文则翻译为中文，中文则保留）",
    "key_points": ["要点1（中文）", "要点2（中文）"],
    "context": "背景说明（中文，包括事件重要性）"
}}
"""

        try:
            # 检测是否有 Claude API key
            if "sk-ant" in self.api_key or "claude" in self.model.lower():
                result = self._call_claude_api(prompt)
            else:
                result = self._call_openai_api(prompt)

            # 解析JSON响应
            import json
            parsed = json.loads(result)
            return {
                "brief": parsed.get("brief", "AI生成摘要"),
                "en_title": parsed.get("en_title", title),
                "cn_title": parsed.get("cn_title", title),
                "key_points": parsed.get("key_points", []),
                "context": parsed.get("context", source)
            }
        except Exception as e:
            console.print(f"[dim]AI摘要生成失败，回退到本地规则: {e}[/dim]")
            return self._generate_local(title, summary, source)

    def _call_claude_api(self, prompt: str) -> str:
        """调用 Claude API"""
        headers = {
            "x-api-key": self.api_key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json"
        }

        data = {
            "model": self.model,
            "max_tokens": 500,
            "messages": [{"role": "user", "content": prompt}]
        }

        response = requests.post(
            "https://api.anthropic.com/v1/messages",
            headers=headers,
            json=data,
            timeout=30
        )
        response.raise_for_status()

        result = response.json()
        return result["content"][0]["text"]

    def _call_openai_api(self, prompt: str) -> str:
        """调用 OpenAI API"""
        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }

        data = {
            "model": self.model if "gpt" in self.model else "gpt-3.5-turbo",
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 500,
            "temperature": 0.3
        }

        response = requests.post(
            "https://api.openai.com/v1/chat/completions",
            headers=headers,
            json=data,
            timeout=30
        )
        response.raise_for_status()

        result = response.json()
        return result["choices"][0]["message"]["content"]


# ============== 新闻获取类 ==============

class NewsAggregator:
    def __init__(self, start_time: Optional[datetime] = None, end_time: Optional[datetime] = None, enable_summary: bool = True):
        self.timezone = pytz.timezone('Asia/Shanghai')

        # 默认时间范围：过去 48 小时
        if end_time is None:
            end_time = datetime.now(self.timezone)
        if start_time is None:
            start_time = end_time - timedelta(hours=48)  # 放宽到48小时

        self.start_time = start_time.astimezone(self.timezone)
        self.end_time = end_time.astimezone(self.timezone)

        self.news_data = {
            "world": [],
            "middle_east": [],
            "economy": [],
            "ai": [],
            "academic": [],
        }

        # 初始化摘要生成器
        self.summarizer = NewsSummarizer() if enable_summary else None

    def is_in_time_range(self, dt: datetime) -> bool:
        """检查时间是否在范围内"""
        if dt.tzinfo is None:
            dt = pytz.UTC.localize(dt)
        dt = dt.astimezone(self.timezone)
        return self.start_time <= dt <= self.end_time

    def fetch_rss(self, url: str, source_name: str, keywords: Optional[List[str]] = None, retries: int = 3) -> List[Dict]:
        """获取 RSS 订阅，带重试机制"""
        items = []
        last_error = None

        for attempt in range(retries):
            try:
                # 使用 requests 处理重定向和 SSL
                headers = {
                    'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
                    'Accept': 'application/rss+xml, application/xml, text/xml, */*',
                }
                response = requests.get(url, headers=headers, timeout=20, allow_redirects=True)
                response.raise_for_status()

                # 解析 RSS
                import io
                feed = feedparser.parse(io.BytesIO(response.content))

                for entry in feed.entries[:25]:  # 限制数量
                    # 解析时间
                    published = entry.get('published', entry.get('updated', ''))
                    try:
                        pub_date = date_parser.parse(published)
                    except:
                        # 无法解析时间，使用当前时间作为后备
                        pub_date = datetime.now(self.timezone)

                    # 检查时间范围
                    if not self.is_in_time_range(pub_date):
                        continue

                    title = entry.get('title', 'No title')
                    summary = entry.get('summary', entry.get('description', ''))
                    link = entry.get('link', '')

                    # 关键词筛选
                    if keywords:
                        content = f"{title} {summary}".lower()
                        if not any(kw.lower() in content for kw in keywords):
                            continue

                    items.append({
                        "title": title,
                        "summary": self._clean_html(summary)[:300],
                        "link": link,
                        "published": pub_date,
                        "source": source_name,
                        "credibility": CREDIBILITY.get(source_name, 3),
                    })

                # 成功获取，直接返回
                return items

            except requests.exceptions.RequestException as e:
                last_error = e
                if attempt < retries - 1:
                    import time
                    time.sleep(1.5)  # 重试前等待
                continue
            except Exception as e:
                last_error = e
                break

        if last_error:
            console.print(f"[yellow]Warning: {source_name} failed after {retries} retries: {last_error}[/yellow]")

        return items

    def fetch_hackernews(self, retries: int = 3) -> List[Dict]:
        """获取 Hacker News 热门 AI 相关话题"""
        items = []
        last_error = None

        for attempt in range(retries):
            try:
                # 获取 top stories
                response = requests.get(f"{APIS['hackernews']}/topstories.json", timeout=15)
                story_ids = response.json()[:30]

                for story_id in story_ids:
                    try:
                        story_resp = requests.get(
                            f"{APIS['hackernews']}/item/{story_id}.json",
                            timeout=5
                        )
                        story = story_resp.json()

                        if not story or 'title' not in story:
                            continue

                        title = story['title']

                        # AI 关键词筛选
                        if not any(kw.lower() in title.lower() for kw in AI_KEYWORDS):
                            continue

                        # 时间检查
                        story_time = datetime.fromtimestamp(story['time'], pytz.UTC)
                        if not self.is_in_time_range(story_time):
                            continue

                        items.append({
                            "title": title,
                            "summary": f"Score: {story.get('score', 0)} | Comments: {story.get('descendants', 0)}",
                            "link": story.get('url', f"https://news.ycombinator.com/item?id={story_id}"),
                            "published": story_time,
                            "source": "Hacker News",
                            "credibility": 4,
                            "score": story.get('score', 0),
                            "comments": story.get('descendants', 0),
                        })
                    except:
                        continue

                return sorted(items, key=lambda x: x.get('score', 0), reverse=True)[:10]

            except Exception as e:
                last_error = e
                if attempt < retries - 1:
                    import time
                    time.sleep(1.5)
                continue

        if last_error:
            console.print(f"[yellow]Warning: Failed to fetch Hacker News after {retries} retries: {last_error}[/yellow]")

        return sorted(items, key=lambda x: x.get('score', 0), reverse=True)[:10]

    def fetch_github_trending(self, retries: int = 3) -> List[Dict]:
        """获取 GitHub AI 相关 trending"""
        items = []
        last_error = None

        for attempt in range(retries):
            try:
                # GitHub API 获取最近更新的 AI 项目
                query = " OR ".join(['"AI"', '"agent"', '"llm"', '"machine learning"'])
                url = "https://api.github.com/search/repositories"
                params = {
                    "q": f"({query}) created:>{(datetime.now() - timedelta(days=7)).strftime('%Y-%m-%d')}",
                    "sort": "updated",
                    "order": "desc",
                    "per_page": 20,
                }

                response = requests.get(url, params=params, timeout=15)
                data = response.json()

                for repo in data.get('items', []):
                    items.append({
                        "title": f"{repo['full_name']}: {repo['description'] or 'No description'}",
                        "summary": f"⭐ {repo['stargazers_count']} | 🍴 {repo['forks_count']} | Language: {repo.get('language', 'N/A')}",
                        "link": repo['html_url'],
                        "published": date_parser.parse(repo['updated_at']),
                        "source": "GitHub",
                        "credibility": 4,
                    })

                return items[:10]

            except Exception as e:
                last_error = e
                if attempt < retries - 1:
                    import time
                    time.sleep(1.5)
                continue

        if last_error:
            console.print(f"[yellow]Warning: Failed to fetch GitHub after {retries} retries: {last_error}[/yellow]")

        return items[:10]

    def fetch_semantic_scholar(self, retries: int = 3) -> List[Dict]:
        """获取 Semantic Scholar 最新 AI 论文"""
        items = []
        last_error = None

        for attempt in range(retries):
            try:
                # 获取最近发布的 AI 相关论文
                url = f"{APIS['semantic_scholar']}/paper/search"
                params = {
                    "query": "artificial intelligence OR large language model OR AI agent",
                    "fields": "title,authors,year,citationCount,url,abstract,venue",
                    "limit": 20,
                    "sort": "publicationDate:desc",
                }

                response = requests.get(url, params=params, timeout=15)

                # 处理限流
                if response.status_code == 429:
                    if attempt < retries - 1:
                        import time
                        time.sleep(3)  # Semantic Scholar 需要更长等待
                        continue
                    else:
                        console.print("[dim]Note: Semantic Scholar rate limited, using arXiv only[/dim]")
                        return []

                response.raise_for_status()
                data = response.json()

                for paper in data.get('data', []):
                    # 检查是否是最近发布的
                    year = paper.get('year', 0)
                    if year < datetime.now().year:
                        continue

                    items.append({
                        "title": paper['title'],
                        "summary": f"Authors: {', '.join([a['name'] for a in paper.get('authors', [])[:3]])} | Citations: {paper.get('citationCount', 0)} | Venue: {paper.get('venue', 'N/A')}",
                        "link": paper.get('url', ''),
                        "published": datetime.now(self.timezone),  # Semantic Scholar 不提供精确日期
                        "source": "Semantic Scholar",
                        "credibility": 5,
                        "citations": paper.get('citationCount', 0),
                    })

                return items[:10]

            except Exception as e:
                last_error = e
                if attempt < retries - 1:
                    import time
                    time.sleep(2)
                continue

        if last_error:
            console.print(f"[yellow]Warning: Failed to fetch Semantic Scholar after {retries} retries: {last_error}[/yellow]")

        return items[:10]

    def _clean_html(self, html: str) -> str:
        """清理 HTML 标签"""
        clean = re.sub(r'<[^>]+>', '', html)
        return clean.strip()

    def deduplicate_and_verify(self, items: List[Dict]) -> Tuple[List[Dict], List[Dict]]:
        """去重并标记验证状态"""
        # 基于标题相似度去重
        unique_items = []
        titles = set()

        for item in sorted(items, key=lambda x: x['credibility'], reverse=True):
            # 简化标题用于比较
            simple_title = re.sub(r'[^\w]', '', item['title'].lower())[:30]

            if simple_title not in titles:
                titles.add(simple_title)
                unique_items.append(item)

        # 标记验证状态
        verified = []
        single_source = []

        # 按标题分组检查多源验证
        title_groups = {}
        for item in unique_items:
            key = re.sub(r'[^\w]', '', item['title'].lower())[:20]
            title_groups.setdefault(key, []).append(item)

        for item in unique_items:
            key = re.sub(r'[^\w]', '', item['title'].lower())[:20]
            group = title_groups.get(key, [])

            if len(group) > 1:
                item['verification'] = 'verified'
                item['sources'] = [g['source'] for g in group]
                verified.append(item)
            elif item['source'] in ['Reuters', 'BBC', 'UN News', 'Federal Reserve', 'ECB']:
                item['verification'] = 'official'
                verified.append(item)
            else:
                item['verification'] = 'single'
                single_source.append(item)

        return verified, single_source

    def collect(self):
        """收集所有新闻"""
        console.print("[blue]正在收集新闻...[/blue]")

        # 世界新闻
        console.print("  [dim]获取世界局势...[/dim]")
        for name, url in RSS_SOURCES["world"].items():
            items = self.fetch_rss(url, name)
            self.news_data["world"].extend(items)

        # 中东新闻（专用RSS + 关键词筛选）
        console.print("  [dim]获取中东局势...[/dim]")
        middle_east_keywords = ['middle east', 'israel', 'palestine', 'gaza', 'iran', 'syria', 'yemen', 'lebanon', '中东', '以色列', '巴勒斯坦', '加沙', '伊朗']
        # 首先获取专用中东源
        for name, url in RSS_SOURCES.get("middle_east", {}).items():
            items = self.fetch_rss(url, name)
            self.news_data["middle_east"].extend(items)
        # 然后从世界新闻中筛选
        for name, url in RSS_SOURCES["world"].items():
            items = self.fetch_rss(url, name, keywords=middle_east_keywords)
            self.news_data["middle_east"].extend(items)

        # 经济新闻
        console.print("  [dim]获取经济政策...[/dim]")
        for category in ["economy_cn", "economy_us", "economy_eu"]:
            for name, url in RSS_SOURCES.get(category, {}).items():
                items = self.fetch_rss(url, name)
                self.news_data["economy"].extend(items)

        # AI 论文
        console.print("  [dim]获取 AI 学术动态...[/dim]")
        for name, url in RSS_SOURCES["ai"].items():
            items = self.fetch_rss(url, name)
            self.news_data["ai"].extend(items)

        # Hacker News
        console.print("  [dim]获取 Hacker News AI 话题...[/dim]")
        hn_items = self.fetch_hackernews()
        self.news_data["ai"].extend(hn_items)

        # GitHub Trending
        console.print("  [dim]获取 GitHub 工程应用...[/dim]")
        gh_items = self.fetch_github_trending()
        self.news_data["ai"].extend(gh_items)

        # Semantic Scholar
        console.print("  [dim]获取学术成果...[/dim]")
        ss_items = self.fetch_semantic_scholar()
        self.news_data["academic"].extend(ss_items)

        # 生成摘要
        if self.summarizer:
            console.print("  [dim]生成新闻摘要...[/dim]")
            self._generate_summaries()

        console.print("[green]收集完成![/green]")

    def _generate_summaries(self):
        """为所有新闻生成摘要"""
        total_items = sum(len(items) for items in self.news_data.values())
        processed = 0

        for category, items in self.news_data.items():
            for item in items:
                if self.summarizer and processed < self.summarizer.max_items:
                    summary_result = self.summarizer.generate_summary(
                        item.get('title', ''),
                        item.get('summary', ''),
                        item.get('source', '')
                    )
                    item['ai_summary'] = summary_result
                    processed += 1
                else:
                    # 超过限制，使用简单的本地规则
                    item['ai_summary'] = self._simple_summary(item)

    def _simple_summary(self, item: Dict) -> Dict[str, str]:
        """简单摘要（无API时）"""
        title = item.get('title', '')
        summary = item.get('summary', '')
        source = item.get('source', '')

        # 检测语言
        is_chinese = any('\u4e00' <= c <= '\u9fff' for c in title)

        if is_chinese:
            return {
                "brief": f"【{source}】{title[:40]}...",
                "en_title": f"[CN] {title}",
                "cn_title": title,
                "key_points": [summary[:60] + "..." if len(summary) > 60 else summary],
                "context": source
            }
        else:
            return {
                "brief": f"【{source}】国际新闻动态",
                "en_title": title,
                "cn_title": f"[EN] {title}",
                "key_points": [summary[:60] + "..." if len(summary) > 60 else summary],
                "context": source
            }

    def generate_report(self) -> str:
        """生成 Markdown 报告"""
        now = datetime.now(self.timezone)

        report = []
        report.append(f"# 晨报 {now.strftime('%Y年%m月%d日')} {self._get_weekday(now)}")
        report.append("")
        report.append(f"> 信息时间范围：{self.start_time.strftime('%m月%d日 %H:%M')} ~ {self.end_time.strftime('%m月%d日 %H:%M')}")
        report.append(f"> 生成时间：{now.strftime('%H:%M')}")

        # 统计
        total = sum(len(v) for v in self.news_data.values())
        verified_count = sum(
            sum(1 for i in items if i.get('verification') in ['verified', 'official'])
            for items in self.news_data.values()
        )
        report.append(f"> 信息统计：共 {total} 条，[已验证/官方] {verified_count} 条")
        report.append("")
        report.append("> 📌 **阅读说明**：每条新闻包含中英文对照标题（EN/CN），📌后为一句话摘要")
        report.append("")
        report.append("---")
        report.append("")

        # 世界局势
        report.append("## 世界局势")
        report.append("")

        # 中东动态
        report.append("### 中东动态")
        if self.news_data["middle_east"]:
            for i, item in enumerate(self.news_data["middle_east"][:8], 1):
                report.append(self._format_item(item, i))
        else:
            report.append("*暂无最新消息*")
        report.append("")

        # 其他地区
        world_others = [i for i in self.news_data["world"]
                       if not any(kw in i['title'].lower() for kw in ['middle east', 'israel', 'gaza', 'iran'])]
        if world_others:
            report.append("### 其他地区")
            for i, item in enumerate(world_others[:5], 1):
                report.append(self._format_item(item, i))
            report.append("")

        # 经济政策
        report.append("## 经济政策")
        report.append("")

        # 按地区分组
        cn_items = [i for i in self.news_data["economy"] if '财新' in i['source']]
        us_items = [i for i in self.news_data["economy"] if any(x in i['source'] for x in ['Fed', 'Treasury', 'CNBC', 'NYT Business'])]
        eu_items = [i for i in self.news_data["economy"] if any(x in i['source'] for x in ['ECB', 'Financial Times'])]

        if cn_items:
            report.append("### 中国")
            for i, item in enumerate(cn_items[:5], 1):
                report.append(self._format_item(item, i))
            report.append("")

        if us_items:
            report.append("### 美国")
            for i, item in enumerate(us_items[:5], 1):
                report.append(self._format_item(item, i))
            report.append("")

        if eu_items:
            report.append("### 欧洲")
            for i, item in enumerate(eu_items[:5], 1):
                report.append(self._format_item(item, i))
            report.append("")

        if not any([cn_items, us_items, eu_items]):
            report.append("*暂无最新经济政策消息*")
            report.append("")

        # AI 技术动态
        report.append("## AI 技术动态")
        report.append("")

        # Agent/Memory 相关
        agent_items = [i for i in self.news_data["ai"]
                      if any(kw in f"{i['title']} {i.get('summary', '')}".lower()
                            for kw in ['agent', 'memory', 'rag', '检索', '智能体'])]
        if agent_items:
            report.append("### AI Agent / Memory")
            for i, item in enumerate(agent_items[:5], 1):
                report.append(self._format_item(item, i, show_score=True))
            report.append("")

        # 其他 AI 动态
        other_ai = [i for i in self.news_data["ai"] if i not in agent_items]
        if other_ai:
            report.append("### 其他 AI 动态")
            for i, item in enumerate(other_ai[:5], 1):
                report.append(self._format_item(item, i, show_score=True))
            report.append("")

        # 学术成果
        if self.news_data["academic"]:
            report.append("### 学术成果")
            for i, item in enumerate(self.news_data["academic"][:5], 1):
                report.append(self._format_item(item, i, show_citations=True))
            report.append("")

        # 可信度说明
        report.append("---")
        report.append("")
        report.append("## 阅读说明")
        report.append("")
        report.append("### 可信度标签")
        report.append("- **[已验证]** - 2+ 独立信源确认")
        report.append("- **[官方发布]** - 政府/机构官方发布（Fed、ECB、UN 等）")
        report.append("- **[单一信源]** - 仅一个来源，建议进一步核实")
        report.append("- **[社区讨论]** - Hacker News/GitHub 社区热度")
        report.append("- **[学术]** - 来自 arXiv/Semantic Scholar 的论文")
        report.append("")
        report.append("### 格式说明")
        report.append("- **📌** - 一句话中文摘要")
        report.append("- **EN:** - 英文原标题/翻译")
        report.append("- **CN:** - 中文标题/翻译")
        report.append("- **•** - 关键要点提取")
        report.append("- **📍** - 来源及背景信息")
        report.append("")
        report.append("---")
        report.append("")
        report.append("*数据来源：BBC、NYT、Al Jazeera、The Guardian、UN、Fed、ECB、CNBC、财新、arXiv、MIT Tech Review、Hacker News、GitHub、Semantic Scholar*")

        return "\n".join(report)

    def _format_item(self, item: Dict, index: int, show_score: bool = False, show_citations: bool = False) -> str:
        """格式化单条新闻 - 包含中英文对照和AI摘要"""
        # 获取AI摘要
        ai_summary = item.get('ai_summary', {})
        brief = ai_summary.get('brief', '')
        en_title = ai_summary.get('en_title', item['title'])
        cn_title = ai_summary.get('cn_title', item['title'])
        key_points = ai_summary.get('key_points', [])
        context = ai_summary.get('context', item.get('source', ''))

        # 验证状态标签
        if item.get('verification') == 'verified':
            tag = "[已验证]"
        elif item.get('verification') == 'official':
            tag = "[官方发布]"
        elif item.get('source') in ['Hacker News', 'GitHub']:
            tag = "[社区讨论]"
        elif item.get('source') in ['Semantic Scholar', 'arXiv']:
            tag = "[学术]"
        else:
            tag = "[单一信源]"

        lines = []

        # 标题行：简要说明 + 标签
        if brief:
            lines.append(f"{index}. **{tag}** 📌 {brief}")
        else:
            lines.append(f"{index}. **{tag}** {item['title']}")

        # 中英文标题对照
        lines.append("   > **EN:** " + (en_title if en_title else item['title']))
        lines.append("   > **CN:** " + (cn_title if cn_title else item['title']))

        # 关键要点
        if key_points:
            for point in key_points[:2]:
                if point and len(point) > 5:
                    lines.append(f"   • {point}")

        # 来源和时间上下文
        extras = []
        if context and context != item.get('source', ''):
            extras.append(f"📍 {context}")
        else:
            extras.append(f"📍 {item.get('source', '')}")

        if show_score and item.get('score'):
            extras.append(f"🔥 {item['score']}")
        if show_score and item.get('comments'):
            extras.append(f"💬 {item['comments']}")
        if show_citations and item.get('citations') is not None:
            extras.append(f"📚 {item['citations']}")

        if extras:
            lines.append(f"   _{' | '.join(extras)}_")

        # 链接
        if item.get('link'):
            lines.append(f"   → [查看原文]({item['link']})")

        return "\n".join(lines)

    def _get_weekday(self, dt: datetime) -> str:
        """获取中文星期"""
        weekdays = ['周一', '周二', '周三', '周四', '周五', '周六', '周日']
        return weekdays[dt.weekday()]

    def save_report(self, content: str, filepath: Optional[str] = None):
        """保存报告到文件"""
        if filepath is None:
            now = datetime.now(self.timezone)
            reports_dir = os.path.join(os.path.dirname(__file__), '..', 'reports')
            os.makedirs(reports_dir, exist_ok=True)
            filepath = os.path.join(reports_dir, f"news_{now.strftime('%Y%m%d')}.md")

        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)

        console.print(f"[green]报告已保存到: {filepath}[/green]")
        return filepath


def main():
    parser = argparse.ArgumentParser(description='晨间新闻聚合')
    parser.add_argument('--today', action='store_true', help='只获取今天的新闻（从 00:00 开始）')
    parser.add_argument('--date', type=str, help='获取指定日期的新闻 (YYYY-MM-DD)')
    parser.add_argument('--start', type=str, help='开始时间 (YYYY-MM-DD)')
    parser.add_argument('--end', type=str, help='结束时间 (YYYY-MM-DD)')
    parser.add_argument('--category', type=str, help='指定类别，逗号分隔 (world,economy,ai)')
    parser.add_argument('--save', action='store_true', help='保存到文件')
    parser.add_argument('--output', type=str, help='输出文件路径')
    parser.add_argument('--no-summary', action='store_true', help='禁用AI摘要（更快，无API调用）')
    parser.add_argument('--ai-summary', action='store_true', help='启用AI摘要（需要 NEW_ANTHROPIC_API_KEY 或 NEW_OPENAI_API_KEY）')

    args = parser.parse_args()

    # 如果启用AI摘要，设置环境变量
    if args.ai_summary:
        os.environ['ENABLE_AI_SUMMARY'] = 'true'

    # 确定时间范围
    tz = pytz.timezone('Asia/Shanghai')
    now = datetime.now(tz)

    if args.today:
        start_time = now.replace(hour=0, minute=0, second=0, microsecond=0)
        end_time = now
    elif args.date:
        date = datetime.strptime(args.date, '%Y-%m-%d')
        start_time = tz.localize(date.replace(hour=0, minute=0))
        end_time = tz.localize(date.replace(hour=23, minute=59))
    elif args.start and args.end:
        start_time = tz.localize(datetime.strptime(args.start, '%Y-%m-%d'))
        end_time = tz.localize(datetime.strptime(args.end, '%Y-%m-%d').replace(hour=23, minute=59))
    else:
        # 默认：前一天 18:00 到当前
        start_time = None
        end_time = None

    # 创建聚合器并收集
    enable_summary = not args.no_summary
    aggregator = NewsAggregator(start_time, end_time, enable_summary=enable_summary)
    aggregator.collect()

    # 生成报告
    report = aggregator.generate_report()

    # 输出
    console.print(Markdown(report))

    # 保存
    if args.save or args.output:
        aggregator.save_report(report, args.output)

    return report


if __name__ == '__main__':
    main()
