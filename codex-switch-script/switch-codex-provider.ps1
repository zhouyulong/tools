# switch-codex-provider.ps1 - Codex 多供应商/模型 一键切换脚本
# 原理：改写 ~/.codex/config.toml 的根键 model_provider / model，并同步环境变量 CODEX_API_KEY
#
# 用法示例（支持短参数：-L=-List -S=-Show -P=-Provider -M=-Model -A=-Add -R=-Remove -Se=-Session -Pr=-Probe）：
#   .\switch-codex-provider.ps1 -List                          # 列出所有已配置供应商（含内置官方 official）
#   .\switch-codex-provider.ps1 -Provider sharedchat           # 切换到 sharedchat：自动探测可用模型，交互选择（选中即写入默认模型）
#   .\switch-codex-provider.ps1 -P sharedchat -M gpt-5.6-sol   # 显式指定模型，跳过探测菜单（不回写默认模型）
#   .\switch-codex-provider.ps1 -P sharedchat -Auto            # 跳过探测菜单，直接用该供应商默认模型（脚本化场景）
#   .\switch-codex-provider.ps1 -Probe sharedchat              # 只探测并列出该供应商可用模型，不切换
#   .\switch-codex-provider.ps1 -P official                    # 切回 OpenAI 官方（ChatGPT 登录走 auth.json；官方模型清单自动读取桌面端缓存）
#   .\switch-codex-provider.ps1 -Show                          # 查看当前生效配置（含模式判定与环境变量残留警告）
#   添加新供应商：
#   .\switch-codex-provider.ps1 -Add myrelay -BaseUrl "https://xxx.com/v1" -ApiKey "sk-xxx" -Model "gpt-5.6-sol" -WireApi responses
#   删除供应商：
#   .\switch-codex-provider.ps1 -Remove myrelay                # 内置官方 official 不可删除
#   按关键词搜索会话（首条消息/标题/工作目录），拿到 session id 后配合 -Se 使用：
#   .\switch-codex-provider.ps1 -Find "关键词"
#   把指定会话（旧对话）钉死的供应商改为当前供应商，使其恢复/继续时走新供应商：
#   .\switch-codex-provider.ps1 -Session <rollout文件路径 或 session id 或 文件名关键字> [-Provider id] [-Model 模型名]
#
# 官方供应商说明：
#   - official 为内置条目（providers.json 缺失时自动补种）：base_url/api_key 留空 = ChatGPT 订阅登录（auth.json）；
#     若给 official 填 api_key 则走 OpenAI API Key 模式。
#   - 切回官方时会：删除 config.toml 根键 model_provider 与 model_catalog_json（恢复内置 provider 与内置模型目录）、
#     删除 DeepSeek/中转遗留的 preferred_auth_method/forced_login_method（订阅登录模式下它们会强制 API Key 登录）、
#     清理会劫持官方请求的 OPENAI_BASE_URL 与残留 Key 环境变量（用户级+当前会话）。
#   - 官方可用模型自动读取桌面端缓存 ~/.codex/models_cache.json（fetched_at 过旧时仅作参考，可 m 手动输入兜底）。
#
# 模型 catalog 说明（桌面端模型列表/显示名的真相源）：
#   - ChatGPT 桌面端按根级 model_catalog_json 显示模型信息。relay 模式下脚本会探测该供应商 /models，
#     生成专属 catalog（~/.codex/model-providers/catalog-<id>.json）并重指根键；切回官方时移除该键还原内置目录。
#   - catalog 条目合并优先级：当前 catalog 条目 > ~/.codex/models.json（DeepSeek 接法等既有目录，保留其
#     model_messages 系统提示模板）> 官方缓存同 slug 条目 > 官方条目克隆（slug/display_name 覆写）。
#   - 探测不到的模型（部分中转站 /models 不全，如手动输入的 id）也会并入 catalog，保证显示正确。
#
# 供应商清单：优先读脚本同目录 providers.json，否则读 ~/.codex/providers.json，可直接手动编辑。

param(
    [Alias('P')]
    [string]$Provider,
    [Alias('M')]
    [string]$Model,
    [Alias('L')]
    [switch]$List,
    [Alias('S')]
    [switch]$Show,
    [Alias('A')]
    [string]$Add,
    [string]$BaseUrl,
    [Alias('K')]
    [string]$ApiKey,
    [string]$Name,
    [string]$WireApi = "responses",
    [Alias('R')]
    [string]$Remove,
    [Alias('Se')]
    [string]$Session,
    [Alias('F')]
    [string]$Find,
    [Alias('Pr')]
    [string]$Probe,
    [switch]$Auto
)

$ErrorActionPreference = "Stop"
# 统一 UTF-8：避免中文输出乱码（python 子进程 + 控制台双端都要设）
$env:PYTHONIOENCODING = "utf-8"
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
$CodexDir    = "$env:USERPROFILE\.codex"
$CodexConfig = "$CodexDir\config.toml"
$ProvFile    = "$CodexDir\providers.json"
if ($PSScriptRoot -and (Test-Path "$PSScriptRoot\providers.json")) {
    $ProvFile = "$PSScriptRoot\providers.json"
}
$OfficialId  = "official"                      # 内置官方供应商 id（providers.json 键名）
$OpenaiBuiltin = "openai"                      # Codex 内置 openai provider 的真实 id（config/rollout 里用的值）
$ModelsCache = "$CodexDir\models_cache.json"   # 桌面端官方模型缓存

function Write-Info($m)    { Write-Host "[信息] " -ForegroundColor Blue -NoNewline;   Write-Host $m }
function Write-Ok($m)      { Write-Host "[成功] " -ForegroundColor Green -NoNewline;  Write-Host $m }
function Write-Warn2($m)   { Write-Host "[警告] " -ForegroundColor Yellow -NoNewline; Write-Host $m }
function Write-Err2($m)    { Write-Host "[错误] " -ForegroundColor Red -NoNewline;    Write-Host $m }

function Build-ProviderCatalog {
    # 生成供应商专属模型 catalog（根级 model_catalog_json 指向它）。
    # 桌面端的模型列表/显示名由该文件决定，切换供应商必须同步重指，否则显示错乱。
    # 条目合并优先级：当前 catalog 条目 > models.json(DeepSeek等既有目录,保留其模板) > 官方缓存条目 > 官方条目克隆
    # 返回 catalog 路径（正斜杠，TOML 中反斜杠是转义字符）；无法构建返回 $null
    param([string]$ProviderId, $p, $ProbeModels, [string]$TargetModel)

    $catDir = "$CodexDir\model-providers"
    if (-not (Test-Path $catDir)) { New-Item -ItemType Directory -Path $catDir -Force | Out-Null }
    $catPath = "$catDir\catalog-$ProviderId.json"

    # 来源1：当前根 catalog 指向的文件（按 slug 携带条目）
    $prior = @{}
    if (Test-Path $CodexConfig) {
        foreach ($line in (Get-Content $CodexConfig -Encoding UTF8)) {
            if ($line -match '^\s*model_catalog_json\s*=\s*"([^"]+)"') {
                $oldCat = $Matches[1]
                if ((Test-Path $oldCat) -and "$oldCat" -ne "$catPath") {
                    try {
                        $pc = Get-Content $oldCat -Raw -Encoding UTF8 | ConvertFrom-Json
                        foreach ($m in $pc.models) { if ($m.slug) { $prior[$m.slug] = $m } }
                    } catch {}
                }
                break
            }
        }
    }
    # 来源2：~/.codex/models.json（DeepSeek 接法等留下的既有目录，含 model_messages 系统提示模板）
    if (Test-Path "$CodexDir\models.json") {
        try {
            $mj = Get-Content "$CodexDir\models.json" -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($m in $mj.models) { if ($m.slug -and -not $prior.ContainsKey($m.slug)) { $prior[$m.slug] = $m } }
        } catch {}
    }
    # 来源3：官方缓存全字段条目（同 slug 模型直接复用官方元数据）
    $official = @{}
    if (Test-Path $ModelsCache) {
        try {
            $oc = Get-Content $ModelsCache -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($m in $oc.models) { if ($m.slug) { $official[$m.slug] = $m } }
        } catch {}
    }

    # 汇总 slug：探测结果 + 目标模型 + 旧目录可携带条目
    $slugs = New-Object System.Collections.Generic.List[string]
    if ($ProbeModels) { foreach ($m in $ProbeModels) { if ($m.id -and -not $slugs.Contains($m.id)) { [void]$slugs.Add($m.id) } } }
    if ($TargetModel -and -not $slugs.Contains($TargetModel)) { [void]$slugs.Insert(0, $TargetModel) }
    foreach ($k in $prior.Keys) { if (-not $slugs.Contains($k)) { [void]$slugs.Add($k) } }
    if ($slugs.Count -eq 0) { return $null }
    $slugs = @($slugs | Sort-Object -Unique)

    # 克隆模板：官方缓存里第一个带 model_messages 的条目（全 schema）
    $tmpl = $null
    foreach ($ov in $official.Values) { if ($ov.model_messages) { $tmpl = $ov; break } }

    $entries = New-Object System.Collections.Generic.List[object]
    $i = 0
    foreach ($s in $slugs) {
        $i++
        $entry = $null
        if ($prior.ContainsKey($s)) {
            $entry = $prior[$s] | ConvertTo-Json -Depth 12 | ConvertFrom-Json   # 深拷贝
        } elseif ($official.ContainsKey($s)) {
            $entry = $official[$s] | ConvertTo-Json -Depth 12 | ConvertFrom-Json
        } elseif ($tmpl) {
            $entry = $tmpl | ConvertTo-Json -Depth 12 | ConvertFrom-Json
            $entry | Add-Member -MemberType NoteProperty -Name slug -Value $s -Force
            $entry | Add-Member -MemberType NoteProperty -Name display_name -Value $s -Force
            $entry | Add-Member -MemberType NoteProperty -Name description -Value "via $ProviderId relay" -Force
        } else { continue }
        $entry | Add-Member -MemberType NoteProperty -Name priority -Value $i -Force
        $entry | Add-Member -MemberType NoteProperty -Name visibility -Value "list" -Force
        [void]$entries.Add($entry)
    }
    if ($entries.Count -eq 0) { return $null }

    $out = [pscustomobject]@{ models = $entries.ToArray() }
    $json = $out | ConvertTo-Json -Depth 12
    # 无 BOM UTF-8：Codex(Rust) 的 JSON 解析不接受 BOM
    [System.IO.File]::WriteAllText($catPath, $json, [System.Text.UTF8Encoding]::new($false))
    return ($catPath -replace '\\', '/')
}

function Test-Interactive {
    # 计划任务/服务等非交互场景自动跳过菜单（Read-Host 从重定向 stdin 读行仍可用）
    return [Environment]::UserInteractive
}

function Get-Providers {
    if (-not (Test-Path $ProvFile)) {
        $seed = [ordered]@{
            official = [ordered]@{
                name = "OpenAI官方"; base_url = ""
                wire_api = "responses"; default_model = "gpt-5.6-terra"; api_key = ""
            }
            sharedchat = [ordered]@{
                name = "sharedchat"; base_url = "https://new.sharedchat.cc/codex"
                wire_api = "responses"; default_model = "gpt-5.6-sol"; api_key = ""
            }
        }
        $seed | ConvertTo-Json -Depth 5 | Set-Content -Path $ProvFile -Encoding UTF8
        Write-Info "已生成供应商清单：$ProvFile"
    }
    $providers = Get-Content $ProvFile -Raw -Encoding UTF8 | ConvertFrom-Json
    # 自动补种内置官方条目（老清单文件升级）
    if (-not $providers.PSObject.Properties[$OfficialId]) {
        $officialEntry = [pscustomobject]@{
            name = "OpenAI官方"; base_url = ""; wire_api = "responses"
            default_model = "gpt-5.6-terra"; api_key = ""
        }
        $providers | Add-Member -MemberType NoteProperty -Name $OfficialId -Value $officialEntry -Force
        Save-Providers $providers
        Write-Info "已自动添加内置官方供应商条目：official（ChatGPT 登录模式）"
    }
    return $providers
}

function Save-Providers($p) {
    $p | ConvertTo-Json -Depth 5 | Set-Content -Path $ProvFile -Encoding UTF8
}

function ConvertTo-TomlString([string]$Value) {
    $escaped = $Value.Replace('\', '\\').Replace('"', '\"')
    return "`"$escaped`""
}

# 与 setup-codex.ps1 相同的 TOML section 合并逻辑：原位替换已有键，缺失键追加
function Set-TomlSectionValues {
    param([string[]]$Lines, [string]$SectionName, $Settings)
    $list = New-Object System.Collections.Generic.List[string]
    foreach ($l in $Lines) { [void]$list.Add($l) }

    $isRoot = [string]::IsNullOrEmpty($SectionName)
    $sectionFound = $isRoot; $startIndex = 0; $endIndex = $list.Count
    if ($isRoot) {
        for ($i = 0; $i -lt $list.Count; $i++) {
            if ($list[$i] -match '^\s*\[.+\]\s*(#.*)?$') { $endIndex = $i; break }
        }
    } else {
        $secPat = '^\s*\[' + [regex]::Escape($SectionName) + '\]\s*(#.*)?$'
        for ($i = 0; $i -lt $list.Count; $i++) {
            if ($list[$i] -match $secPat) {
                $sectionFound = $true; $startIndex = $i + 1; $endIndex = $list.Count
                for ($j = $startIndex; $j -lt $list.Count; $j++) {
                    if ($list[$j] -match '^\s*\[.+\]\s*(#.*)?$') { $endIndex = $j; break }
                }
                break
            }
        }
    }
    if (-not $sectionFound) {
        if ($list.Count -gt 0 -and -not [string]::IsNullOrWhiteSpace($list[$list.Count - 1])) { [void]$list.Add("") }
        [void]$list.Add("[$SectionName]")
        foreach ($k in $Settings.Keys) { [void]$list.Add("$k = $($Settings[$k])") }
        return $list.ToArray()
    }
    $updated = @{}
    for ($i = $startIndex; $i -lt $endIndex; $i++) {
        foreach ($k in $Settings.Keys) {
            if ($list[$i] -match ('^(\s*)' + [regex]::Escape($k) + '\s*=.*$')) {
                $list[$i] = "$($Matches[1])$k = $($Settings[$k])"
                $updated[$k] = $true; break
            }
        }
    }
    $off = 0
    foreach ($k in $Settings.Keys) {
        if (-not $updated.ContainsKey($k)) {
            $list.Insert($endIndex + $off, "$k = $($Settings[$k])"); $off++
        }
    }
    return $list.ToArray()
}

# 删除 TOML 根区域（首个 [section] 之前）的指定键：切回官方时用于移除 model_provider 选择器
function Remove-TomlRootKeys {
    param([string[]]$Lines, [string[]]$Keys)
    $out = New-Object System.Collections.Generic.List[string]
    $inRoot = $true
    foreach ($l in $Lines) {
        if ($inRoot -and $l -match '^\s*\[.+\]\s*(#.*)?$') { $inRoot = $false }
        $drop = $false
        if ($inRoot) {
            foreach ($k in $Keys) {
                if ($l -match ('^\s*' + [regex]::Escape($k) + '\s*=')) { $drop = $true; break }
            }
        }
        if (-not $drop) { [void]$out.Add($l) }
    }
    return $out.ToArray()
}

function Backup-Config {
    if (Test-Path $CodexConfig) {
        $bak = "$CodexConfig.backup.switch_$(Get-Date -Format 'yyyyMMdd_HHmmss')"
        Copy-Item $CodexConfig $bak
        Write-Info "已备份配置到：$bak"
    }
}

# ---------- 模型探测 ----------

# 官方模型兜底清单（models_cache.json 不可用时使用，2026-09 校准）
$script:OfficialFallback = @(
    [pscustomobject]@{ id = "gpt-5.6-terra";     label = "GPT-5.6-Terra" }
    [pscustomobject]@{ id = "gpt-5.6-luna";      label = "GPT-5.6-Luna" }
    [pscustomobject]@{ id = "gpt-reserve";       label = "GPT-Reserve" }
    [pscustomobject]@{ id = "gpt-5.5";           label = "GPT-5.5" }
    [pscustomobject]@{ id = "gpt-5.4-mini";      label = "GPT-5.4-Mini" }
    [pscustomobject]@{ id = "codex-auto-review"; label = "Codex Auto Review" }
)

function Get-AvailableModels {
    # 返回 @{ Models = @([pscustomobject]@{id;label}); Source = "..." }；探测失败返回 $null
    param([string]$ProviderId, $p)
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch {}

    if ($ProviderId -eq $OfficialId) {
        # 官方：读桌面端模型缓存（ChatGPT 登录态无法直接列模型，缓存由桌面端自动刷新）
        if (Test-Path $ModelsCache) {
            try {
                $cache = Get-Content $ModelsCache -Raw -Encoding UTF8 | ConvertFrom-Json
                $models = @($cache.models | ForEach-Object {
                    [pscustomobject]@{ id = $_.slug; label = $_.display_name }
                } | Where-Object { $_.id })
                if ($models.Count -gt 0) {
                    return @{
                        Models = $models
                        Source = "桌面端缓存 models_cache.json（fetched_at $($cache.fetched_at)，过期数据仅供参考，可 m 手动输入）"
                    }
                }
            } catch {}
        }
        return @{ Models = $script:OfficialFallback; Source = "内置已知官方模型清单（models_cache.json 不可用）" }
    }

    # relay：GET {base_url}/models，未以 /v1 结尾的再兜底 {域名根}/v1/models
    $b = ($p.base_url -replace '/+$', '')
    if (-not $b) { return $null }
    $urls = @("$b/models")
    if ($b -notmatch '/v1$') {
        $root = $b -replace '^(https?://[^/]+).*$', '$1'
        $urls += "$root/v1/models"
    }
    foreach ($u in $urls) {
        $resp = $null
        # 引擎1：curl.exe——PS5.1/.NET 的 TLS 指纹会被 Cloudflare 类防护挑战(403)，curl 指纹可通过
        $curlExe = Get-Command curl.exe -ErrorAction SilentlyContinue
        if ($curlExe) {
            $curlOut = & curl.exe -s -m 12 -A "codex-switch-script/1.0" -H "Authorization: Bearer $($p.api_key)" $u
            if ($LASTEXITCODE -eq 0 -and $curlOut) {
                $raw = ($curlOut -join "`n").Trim()
                if ($raw -match '^\s*[\{\[]') {
                    try { $resp = ConvertFrom-Json $raw } catch { $resp = $null }
                }
            }
        }
        # 引擎2：Invoke-RestMethod 兜底（无 curl 时或 curl 失败时）
        if (-not $resp) {
            try {
                $resp = Invoke-RestMethod -Uri $u -Headers @{ Authorization = "Bearer $($p.api_key)" } `
                    -TimeoutSec 12 -UserAgent "codex-switch-script/1.0"
            } catch { $resp = $null }
        }
        if ($resp) {
            $list = $null
            if ($resp -is [System.Array]) { $list = $resp }
            elseif ($resp.PSObject.Properties['data']) { $list = $resp.data }
            if ($list) {
                $models = @($list | ForEach-Object {
                    $id = $null
                    if ($_ -is [string]) { $id = $_ }
                    elseif ($_.PSObject.Properties['id']) { $id = $_.id }
                    if ($id) { [pscustomobject]@{ id = "$id"; label = $null } }
                } | Where-Object { $_.id -and $_.id -notmatch '(?i)embedding|whisper|tts|speech|dall-e|moderation|rerank' })
                $models = @($models | Sort-Object -Property id -Unique)
                if ($models.Count -gt 0) { return @{ Models = $models; Source = $u } }
            }
        }
    }
    return $null
}

function Show-ModelMenu {
    # 交互选择模型；返回 @{ Model = "..."; Cancelled = $bool }
    param($p, $probe)
    Write-Host ""
    Write-Info ("可用模型来源：" + $probe.Source)
    Write-Host ("可用模型（共 {0} 个）：" -f $probe.Models.Count) -ForegroundColor Cyan
    $i = 0
    foreach ($m in $probe.Models) {
        $i++
        $lbl = if ($m.label) { "  ($($m.label))" } else { "" }
        Write-Host ("{0,4}. {1}{2}" -f $i, $m.id, $lbl)
    }
    $def = if ($p.default_model) { $p.default_model } else { "(未设置)" }
    Write-Host ("   d. 使用默认模型 {0}" -f $def) -ForegroundColor DarkGray
    Write-Host "   m. 手动输入模型 ID" -ForegroundColor DarkGray
    Write-Host "   0. 取消切换" -ForegroundColor DarkGray
    while ($true) {
        $choice = (Read-Host "请选择（编号 / d / m / 0）")
        $choice = "$choice".Trim()
        if ($choice -eq "0") { return @{ Model = ""; Cancelled = $true } }
        if ($choice -eq "d") {
            if ($p.default_model) { return @{ Model = $p.default_model; Cancelled = $false } }
            Write-Warn2 "该供应商未设置默认模型，请改用编号或 m 手动输入"
            continue
        }
        if ($choice -eq "m") {
            $mid = (Read-Host "请输入模型 ID")
            $mid = "$mid".Trim()
            if ($mid) { return @{ Model = $mid; Cancelled = $false } }
            Write-Warn2 "模型 ID 不能为空"
            continue
        }
        if ($choice -match '^\d+$') {
            $n = [int]$choice
            if ($n -ge 1 -and $n -le $probe.Models.Count) {
                return @{ Model = $probe.Models[$n - 1].id; Cancelled = $false }
            }
        }
        Write-Warn2 "无效输入：$choice（请输入 1-$($probe.Models.Count) 的编号，或 d / m / 0）"
    }
}

# ---------- 动作分发 ----------

if ($List) {
    $providers = Get-Providers
    $current = ""
    if (Test-Path $CodexConfig) {
        foreach ($line in (Get-Content $CodexConfig -Encoding UTF8)) {
            if ($line -match '^\s*model_provider\s*=\s*"([^"]+)"') { $current = $Matches[1]; break }
        }
    }
    if (-not $current -or $current -eq $OpenaiBuiltin) { $current = $OfficialId }
    Write-Host ""
    Write-Host "已配置的 Codex 供应商（* 为当前生效）：" -ForegroundColor Cyan
    foreach ($prop in $providers.PSObject.Properties) {
        $p = $prop.Value
        $mark = if ($prop.Name -eq $current) { "*" } else { " " }
        $urlDisp = if ($p.base_url) { $p.base_url } else { "(内置 api.openai.com)" }
        $keyMask = if ($p.api_key) { $p.api_key.Substring(0, [Math]::Min(8, $p.api_key.Length)) + "..." } else { "(ChatGPT登录/未设Key)" }
        Write-Host ("$mark {0,-14} {1}  协议={2}  默认模型={3}  Key={4}" -f `
            $prop.Name, $urlDisp, $p.wire_api, $p.default_model, $keyMask)
    }
    Write-Host ""
    return
}

if ($Show) {
    Write-Host ""
    Write-Info "当前 config.toml 关键配置："
    $curProvider = ""
    $curModel = ""
    if (Test-Path $CodexConfig) {
        Get-Content $CodexConfig -Encoding UTF8 | ForEach-Object {
            if ($_ -match '^\s*model_provider\s*=\s*"([^"]+)"') { $curProvider = $Matches[1] }
            if ($_ -match '^\s*model\s*=\s*"([^"]+)"') { $curModel = $Matches[1] }
            if ($_ -match '^\s*(model|model_provider|model_reasoning_effort|model_catalog_json)\s*=' -or $_ -match '^\[model_providers\.') {
                Write-Host "  $_"
            }
        }
    } else { Write-Warn2 "未找到 $CodexConfig" }
    if (-not $curProvider -or $curProvider -eq $OpenaiBuiltin) {
        Write-Info "当前模式：OpenAI 官方（model_provider 未设/内置，认证优先走 auth.json ChatGPT 登录）"
    } else {
        Write-Info "当前模式：自定义供应商 $curProvider"
    }
    # model_catalog_json 诊断：桌面端模型列表/显示名由此文件决定
    $catKey = $null
    foreach ($line in (Get-Content $CodexConfig -Encoding UTF8)) {
        if ($line -match '^\s*model_catalog_json\s*=\s*"([^"]+)"') { $catKey = $Matches[1]; break }
    }
    if ($catKey) {
        if (Test-Path $catKey) {
            try {
                $cc = Get-Content $catKey -Raw -Encoding UTF8 | ConvertFrom-Json
                $catSlugs = @($cc.models | ForEach-Object { $_.slug })
                Write-Info ("模型 catalog：{0}（{1} 个模型）" -f $catKey, $catSlugs.Count)
                if ($curModel -and $catSlugs -notcontains $curModel) {
                    Write-Warn2 "当前模型 '$curModel' 不在 catalog 中：桌面端模型显示会不准。重跑 -P $curProvider 刷新 catalog"
                }
            } catch { Write-Warn2 "catalog 文件解析失败：$catKey（$_）" }
        } else {
            Write-Warn2 "model_catalog_json 指向的文件不存在：$catKey（桌面端会静默回退内置元数据，显示不准）"
        }
    } elseif ($curProvider -and $curProvider -ne $OpenaiBuiltin) {
        Write-Warn2 "自定义供应商模式但未设 model_catalog_json：桌面端模型显示会不准。重跑 -P $curProvider 生成 catalog"
    }
    # auth.json 登录态
    if (Test-Path "$CodexDir\auth.json") {
        try {
            $auth = Get-Content "$CodexDir\auth.json" -Raw -Encoding UTF8 | ConvertFrom-Json
            Write-Info ("auth.json: auth_mode={0}  ChatGPT登录有效={1}" -f $auth.auth_mode, [bool]$auth.tokens)
        } catch { Write-Warn2 "auth.json 解析失败：$_" }
    } else {
        Write-Warn2 "未找到 auth.json（官方 ChatGPT 登录模式需要；如需官方 API Key 模式请给 official 配 api_key）"
    }
    # 环境变量状态与劫持检查
    $envKey = [Environment]::GetEnvironmentVariable("CODEX_API_KEY", "User")
    if ($envKey) { Write-Info ("CODEX_API_KEY(User) = " + $envKey.Substring(0, [Math]::Min(8, $envKey.Length)) + "...") }
    else { Write-Info "用户级环境变量 CODEX_API_KEY 未设置（官方 ChatGPT 登录模式属正常）" }
    $envBaseUrl = [Environment]::GetEnvironmentVariable("OPENAI_BASE_URL", "User")
    if ($envBaseUrl) {
        if (-not $curProvider -or $curProvider -eq $OpenaiBuiltin) {
            Write-Warn2 "官方模式下检测到残留 OPENAI_BASE_URL(User)=$envBaseUrl，会劫持官方请求！执行 -P official 可自动清理"
        } else {
            Write-Info "OPENAI_BASE_URL(User) = $envBaseUrl"
        }
    }
    Write-Host ""
    return
}

if ($Probe) {
    $probeTarget = $Probe
    if ($probeTarget -eq $OpenaiBuiltin) { $probeTarget = $OfficialId }
    $providers = Get-Providers
    $prop = $providers.PSObject.Properties[$probeTarget]
    if (-not $prop) { Write-Err2 "供应商 '$Probe' 不存在。用 -List 查看已有供应商。"; return }
    Write-Info "正在探测可用模型 ..."
    # 注意：局部变量不能叫 $probe——与 param 的 [string]$Probe 大小写不敏感冲突，hashtable 会被强转成字符串
    $probeResult = Get-AvailableModels -ProviderId $probeTarget -p $prop.Value
    if (-not $probeResult -or $probeResult.Models.Count -eq 0) {
        Write-Err2 "探测失败：该供应商不支持模型列表接口（或网络/鉴权异常）。切换时可 m 手动输入模型 ID 兜底。"
        return
    }
    Write-Ok ("探测到 {0} 个模型（{1}）" -f $probeResult.Models.Count, $probeResult.Source)
    $i = 0
    foreach ($m in $probeResult.Models) {
        $i++
        $lbl = if ($m.label) { "  ($($m.label))" } else { "" }
        Write-Host ("{0,4}. {1}{2}" -f $i, $m.id, $lbl)
    }
    Write-Host ""
    Write-Info "执行 -P $probeTarget 切换时会出现选择菜单（加 -Auto 跳过菜单直接用默认模型）"
    return
}

if ($Remove) {
    if ($Remove -eq $OfficialId -or $Remove -eq $OpenaiBuiltin) { Write-Err2 "内置官方供应商不可删除"; return }
    $providers = Get-Providers
    if (-not $providers.PSObject.Properties[$Remove]) { Write-Err2 "供应商 '$Remove' 不存在"; return }
    $providers.PSObject.Properties.Remove($Remove)
    Save-Providers $providers
    Write-Ok "已删除供应商：$Remove"
    return
}

if ($Add) {
    if (-not $BaseUrl -or -not $ApiKey) { Write-Err2 "添加供应商需要同时提供 -BaseUrl 和 -ApiKey"; return }
    $providers = Get-Providers
    $entry = [ordered]@{
        name = if ($Name) { $Name } else { $Add }
        base_url = $BaseUrl.TrimEnd('/')
        wire_api = $WireApi
        default_model = if ($Model) { $Model } else { "" }
        api_key = $ApiKey
    }
    if ($providers.PSObject.Properties[$Add]) { Write-Warn2 "供应商 '$Add' 已存在，将覆盖" }
    $providers | Add-Member -MemberType NoteProperty -Name $Add -Value $entry -Force
    Save-Providers $providers
    Write-Ok "已保存供应商：$Add -> $BaseUrl"
    Write-Info "执行 .\switch-codex-provider.ps1 -Provider $Add 即可切换"
    return
}

if ($Provider) {
    if ($Provider -eq $OpenaiBuiltin) {
        $Provider = $OfficialId
        Write-Info "已映射：openai -> 内置官方供应商 official"
    }
    $providers = Get-Providers
    $prop = $providers.PSObject.Properties[$Provider]
    if (-not $prop) {
        Write-Err2 "供应商 '$Provider' 不存在。用 -List 查看已有供应商，或用 -Add 添加。"
        return
    }
    $p = $prop.Value
    $isOfficial = ($Provider -eq $OfficialId)

    # 1) 确定目标模型：-Model 显式指定 > 自动探测交互 > 默认模型
    #    探测除供菜单选择外，还为模型 catalog 收集清单：任何切换都执行（-Model/-Auto 时静默，不发交互）；
    #    失败不阻断切换，仅 catalog 缺探测清单（Build-ProviderCatalog 会用旧目录/models.json/官方缓存兜底）
    $targetModel = $Model
    $probeResult = Get-AvailableModels -ProviderId $Provider -p $p
    if ($probeResult -and $probeResult.Models.Count -gt 0) {
        Write-Ok ("探测到 {0} 个可用模型" -f $probeResult.Models.Count)
    } else {
        $probeResult = $null
        Write-Warn2 "探测失败（接口不支持 /models 或网络/鉴权异常），不影响切换，仅模型 catalog 清单变少"
    }
    if (-not $targetModel -and -not $Auto) {
        if ($probeResult -and $probeResult.Models.Count -gt 0) {
            $sel = Show-ModelMenu -p $p -probe $probeResult
            if ($sel.Cancelled) { Write-Info "已取消切换"; return }
            $targetModel = $sel.Model
        } elseif (Test-Interactive) {
            Write-Warn2 "自动探测失败（接口不支持 /models 或网络/鉴权异常），可手动输入模型 ID"
            $hint = if ($p.default_model) { "（直接回车使用默认模型 $($p.default_model)）" } else { "" }
            $mid = (Read-Host "请输入模型 ID$hint")
            $mid = "$mid".Trim()
            if ($mid) { $targetModel = $mid }
            elseif ($p.default_model) {
                $targetModel = $p.default_model
                Write-Info "未输入，使用默认模型：$targetModel"
            }
        } else {
            Write-Warn2 "非交互环境且探测失败，回退使用默认模型"
        }
    }
    if (-not $targetModel) { $targetModel = $p.default_model }
    if (-not $targetModel) {
        Write-Err2 "未指定模型：请用 -Model 传入，或在探测菜单中选择/手动输入，或在 providers.json 中为该供应商设置 default_model"
        return
    }

    # 2) 交互选定的模型回写为该供应商默认模型（-Model 显式指定视为临时覆盖，不回写）
    if (-not $Model -and "$($p.default_model)" -ne $targetModel) {
        $p.default_model = $targetModel
        Save-Providers $providers
        Write-Ok "默认模型已更新：$Provider.default_model = $targetModel（写入 $ProvFile）"
    }

    Backup-Config

    $lines = @()
    if (Test-Path $CodexConfig) { $lines = Get-Content $CodexConfig -Encoding UTF8 }

    if ($isOfficial) {
        # ===== 官方模式：删 model_provider + model_catalog_json 根键（恢复内置 provider 与内置模型目录）+ 设 model =====
        $lines = Remove-TomlRootKeys -Lines $lines -Keys @("model_provider", "model_catalog_json")
        # DeepSeek/中转接法安装脚本会遗留 preferred_auth_method="apikey"/forced_login_method="api"，
        # 这两个根键强制 Codex 只能 API Key 登录、禁用 ChatGPT 账号（订阅）登录；
        # 订阅登录模式（official 未配 api_key）下必须删除，官方 API Key 模式可保留
        if (-not $p.api_key) {
            $removedAuth = @()
            foreach ($k in @("preferred_auth_method", "forced_login_method")) {
                if ($lines | Where-Object { $_ -match ('^\s*' + [regex]::Escape($k) + '\s*=') }) { $removedAuth += $k }
            }
            if ($removedAuth.Count -gt 0) {
                $lines = Remove-TomlRootKeys -Lines $lines -Keys $removedAuth
                Write-Ok ("已删除遗留认证根键 {0}（该键强制 API Key 登录、禁用 ChatGPT 账号登录）" -f ($removedAuth -join ' / '))
            }
        }
        $rootSettings = [ordered]@{
            model = ConvertTo-TomlString $targetModel
        }
        $lines = Set-TomlSectionValues -Lines $lines -SectionName "" -Settings $rootSettings
        # 无 BOM 写入：Codex(Rust) 的 TOML 解析器不接受 BOM，PS5.1 的 Set-Content -Encoding UTF8 会写 BOM
        [System.IO.File]::WriteAllLines($CodexConfig, [string[]]$lines, [System.Text.UTF8Encoding]::new($false))
        Write-Ok "config.toml 已更新：model_provider=(内置官方), model=$targetModel"

        # 环境变量：OPENAI_BASE_URL 必清（否则劫持官方请求）；Key 按模式处理
        [Environment]::SetEnvironmentVariable("OPENAI_BASE_URL", $null, "User")
        $env:OPENAI_BASE_URL = $null
        if ($p.api_key) {
            [Environment]::SetEnvironmentVariable("CODEX_API_KEY", $p.api_key, "User")
            [Environment]::SetEnvironmentVariable("OPENAI_API_KEY", $p.api_key, "User")
            $env:CODEX_API_KEY = $p.api_key
            $env:OPENAI_API_KEY = $p.api_key
            Write-Ok "官方 API Key 模式：CODEX_API_KEY / OPENAI_API_KEY 已设置"
        } else {
            [Environment]::SetEnvironmentVariable("CODEX_API_KEY", $null, "User")
            [Environment]::SetEnvironmentVariable("OPENAI_API_KEY", $null, "User")
            $env:CODEX_API_KEY = $null
            $env:OPENAI_API_KEY = $null
            Write-Ok "ChatGPT 登录模式：已清理残留 Key/BaseUrl 环境变量（认证走 auth.json）"
        }
        if (-not (Test-Path "$CodexDir\auth.json") -and -not $p.api_key) {
            Write-Warn2 "未检测到 auth.json 且未配置 api_key：请先运行 codex login 完成 ChatGPT 登录，或给 official 配置 api_key"
        }
    } else {
        # ===== relay 模式：写供应商 section + 根选择器 =====
        $providerSettings = [ordered]@{
            name = ConvertTo-TomlString $p.name
            base_url = ConvertTo-TomlString $p.base_url
            wire_api = ConvertTo-TomlString $p.wire_api
            env_key = ConvertTo-TomlString "CODEX_API_KEY"
            supports_websockets = "false"
        }
        $lines = Set-TomlSectionValues -Lines $lines -SectionName "model_providers.$Provider" -Settings $providerSettings

        $rootSettings = [ordered]@{
            model_provider = ConvertTo-TomlString $Provider
            model = ConvertTo-TomlString $targetModel
        }
        # 模型 catalog：桌面端模型列表/显示名由根级 model_catalog_json 决定，切供应商必须同步重指
        $probeModels = $null
        if ($probeResult) { $probeModels = $probeResult.Models }
        $catPath = Build-ProviderCatalog -ProviderId $Provider -p $p -ProbeModels $probeModels -TargetModel $targetModel
        if ($catPath) {
            $rootSettings["model_catalog_json"] = ConvertTo-TomlString $catPath
            Write-Ok "模型 catalog 已更新：$catPath"
        } else {
            Write-Warn2 "模型 catalog 生成失败，保留原 model_catalog_json（桌面端模型显示可能不准）"
        }
        $lines = Set-TomlSectionValues -Lines $lines -SectionName "" -Settings $rootSettings
        [System.IO.File]::WriteAllLines($CodexConfig, [string[]]$lines, [System.Text.UTF8Encoding]::new($false))
        Write-Ok "config.toml 已更新：model_provider=$Provider, model=$targetModel"

        if ($p.api_key) {
            [Environment]::SetEnvironmentVariable("CODEX_API_KEY", $p.api_key, "User")
            [Environment]::SetEnvironmentVariable("OPENAI_API_KEY", $p.api_key, "User")
            [Environment]::SetEnvironmentVariable("OPENAI_BASE_URL", $p.base_url, "User")
            $env:CODEX_API_KEY = $p.api_key
            $env:OPENAI_API_KEY = $p.api_key
            $env:OPENAI_BASE_URL = $p.base_url
            Write-Ok "环境变量 CODEX_API_KEY / OPENAI_API_KEY / OPENAI_BASE_URL 已更新"
        } else {
            Write-Warn2 "该供应商未配置 api_key，请编辑 $ProvFile 补充，否则认证会失败"
        }
    }

    Write-Host ""
    $destDisp = if ($isOfficial) { "OpenAI 官方" } else { "$Provider ($($p.base_url))" }
    Write-Ok "切换完成：$destDisp 模型=$targetModel"
    Write-Warn2 "注意：需重启终端 / ChatGPT 桌面端后新配置才会生效"
    return
}

# ---------- 会话搜索：按关键词（首条消息/标题/预览/工作目录）在 threads 注册表中找 session id ----------
if ($Find) {
    $db = "$CodexDir\state_5.sqlite"
    if (-not (Test-Path $db)) { Write-Err2 "未找到 $db"; return }
    $tmpPy = Join-Path $env:TEMP "codex-find-threads.py"
    @'
import sqlite3, sys
kw = "%" + sys.argv[1] + "%"
con = sqlite3.connect(sys.argv[2], timeout=10)
rows = con.execute("""
    SELECT id, model_provider, model, archived, cwd,
           substr(COALESCE(first_user_message,''),1,60), substr(COALESCE(title,''),1,40),
           datetime(recency_at, 'unixepoch', 'localtime')
    FROM threads
    WHERE first_user_message LIKE ? OR title LIKE ? OR preview LIKE ? OR cwd LIKE ?
    ORDER BY recency_at DESC LIMIT 20
""", (kw, kw, kw, kw)).fetchall()
for r in rows:
    print("ID:", r[0])
    print("   provider:", r[1], "| model:", r[2], "| archived:", r[3], "| 最近活跃:", r[7])
    print("   cwd:", r[4])
    print("   首条消息:", r[5].replace(chr(10), " "))
con.close()
'@ | Set-Content -Path $tmpPy -Encoding UTF8
    $eap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $found = $false
    try {
        foreach ($cand in @(@('python', @()), @('py', @('-3')), @('python3', @()))) {
            if (-not (Get-Command $cand[0] -ErrorAction SilentlyContinue)) { continue }
            $r = (& $cand[0] @($cand[1]) $tmpPy $Find $db 2>&1 | Out-String)
            if ($LASTEXITCODE -eq 0) {
                $found = $true
                if ($r.Trim()) { Write-Host $r.Trim() }
                else { Write-Warn2 "没有匹配关键词的会话：$Find" }
                break
            }
        }
    } finally {
        $ErrorActionPreference = $eap
        Remove-Item $tmpPy -ErrorAction SilentlyContinue
    }
    if (-not $found) { Write-Err2 "查询失败（python 不可用？），输出：$r" }
    return
}

# ---------- 会话供应商改写：把 rollout 会话文件里钉死的 model_provider 改为目标供应商 ----------
if ($Session) {
    # 目标供应商：-Provider 指定，否则取 config.toml 当前值
    $targetId = $Provider
    if (-not $targetId) {
        if (-not (Test-Path $CodexConfig)) { Write-Err2 "未找到 $CodexConfig，请用 -Provider 指定目标供应商"; return }
        foreach ($line in (Get-Content $CodexConfig -Encoding UTF8)) {
            if ($line -match '^\s*model_provider\s*=\s*"([^"]+)"') { $targetId = $Matches[1]; break }
        }
    }
    if (-not $targetId) { Write-Err2 "无法确定目标供应商"; return }
    # 会话 rollout 里的 provider id 必须是 Codex 真实 id：官方对应内置 openai，不是 official
    if ($targetId -eq $OfficialId) { $targetId = $OpenaiBuiltin; Write-Info "会话改写目标映射：official -> $OpenaiBuiltin" }

    # 解析会话文件：直接路径 / session id / 文件名关键字
    # 注意：必须用 @() 强制数组化——管道只有1个结果时 PS 会退化成标量字符串，
    #       导致 $files[0] 取到字符串首字符 "C" 而不是完整路径
    $files = @()
    if (Test-Path $Session) {
        $files += (Resolve-Path $Session).Path
    } else {
        $files = @(Get-ChildItem "$CodexDir\sessions", "$CodexDir\archived_sessions" -Recurse -Filter "*.jsonl" -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "*$Session*" } | Select-Object -ExpandProperty FullName)
    }
    if (-not $files) { Write-Err2 "未找到匹配的会话文件：$Session"; return }
    if ($files.Count -gt 1) {
        Write-Warn2 "匹配到 $($files.Count) 个文件，请用更精确的 id 或完整路径："
        $files | ForEach-Object { Write-Host "  $_" }
        return
    }

    $path = $files[0]
    $stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
    Copy-Item $path "$path.bak-switch-$stamp"
    $oldIds = New-Object System.Collections.Generic.HashSet[string]
    $out = New-Object System.Collections.Generic.List[string]
    foreach ($line in [System.IO.File]::ReadAllLines($path)) {
        $newLine = $line
        # 替换 model_provider / model_provider_id 的值（会话 turn_context 快照）
        # 注意：引号必须放进捕获组，否则 [regex]::Replace 重建字符串时会丢掉 key 的前引号
        foreach ($key in @('model_provider_id', 'model_provider')) {
            $pat = '("' + $key + '")(\s*:\s*")([^"]*)(")'
            if ($newLine -match $pat) {
                [void]$oldIds.Add($Matches[3])
                $newLine = [regex]::Replace($newLine, $pat, ('$1$2' + $targetId + '$4'))
            }
        }
        # 可选：同时替换模型名
        if ($Model) {
            $newLine = [regex]::Replace($newLine, '("model")(\s*:\s*")[^"]*(")', ('$1$2' + $Model + '$3'))
        }
        [void]$out.Add($newLine)
    }

    if ($oldIds.Count -eq 0 -or ($oldIds.Count -eq 1 -and $oldIds.Contains($targetId))) {
        Write-Warn2 "会话 rollout 中没有需要改写的供应商记录（或已是指定供应商）"
        Remove-Item "$path.bak-switch-$stamp" -ErrorAction SilentlyContinue
    } else {
        [System.IO.File]::WriteAllLines($path, [string[]]$out.ToArray(), [System.Text.UTF8Encoding]::new($false))
        Write-Ok "会话 rollout 已改写：$($oldIds -join ',') -> $targetId$(if ($Model) { "，模型 -> $Model" })"
        Write-Info "文件：$path"
        Write-Info "已备份：$path.bak-switch-$stamp"
    }

    # 同步桌面端会话注册表 state_5.sqlite 的 threads 表：
    # 桌面端恢复会话时读的是这里的 model_provider / model 列，不是 rollout 文件
    # 双条件匹配：桌面端运行中可能改写 rollout_path 的表示，按文件名中提取的线程 id 兜底
    $db = "$CodexDir\state_5.sqlite"
    if (Test-Path $db) {
        $leaf = Split-Path $path -Leaf
        $threadId = ""
        if ($leaf -match '([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.jsonl$') {
            $threadId = $Matches[1]
        }
        $pyCode = @'
import sqlite3, sys
db, path, tid, provider, model = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
if model == "-":
    model = ""
con = sqlite3.connect(db, timeout=10)
try:
    cur = con.cursor()
    sets, args = ["model_provider = ?"], [provider]
    if model:
        sets.append("model = ?")
        args.append(model)
    args += [tid, path]
    cur.execute("UPDATE threads SET " + ", ".join(sets) + " WHERE id = ? OR rollout_path = ?", args)
    con.commit()
    print("updated=" + str(cur.rowcount))
finally:
    con.close()
'@
        # 解析可用的 python：依次尝试 python / py -3 / python3
        # （Windows 上 python 可能是微软商店假别名：存在但只往 stderr 打印提示，需换下一个候选）
        # python 代码写入临时 .py 文件执行：-c 传多行代码在 PS5.1 下有引号/换行转义风险
        $tmpPy = Join-Path $env:TEMP "codex-sync-threads.py"
        Set-Content -Path $tmpPy -Value $pyCode -Encoding ASCII
        $eap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        $syncOk = $false; $lastOut = ""; $lastCode = $null
        try {
            foreach ($cand in @(@('python', @()), @('py', @('-3')), @('python3', @()))) {
                if (-not (Get-Command $cand[0] -ErrorAction SilentlyContinue)) { continue }
                # PS5.1 会丢弃空字符串参数导致 sys.argv 越界，空 Model 用 "-" 占位
                $modelArg = if ($Model) { $Model } else { "-" }
                $allArgs = @($cand[1]) + @($tmpPy, $db, $path, $threadId, $targetId, $modelArg)
                $r = (& $cand[0] @allArgs 2>&1 | Out-String)
                $code = $LASTEXITCODE
                if ($code -eq 0 -and "$r" -match 'updated=(\d+)') {
                    $syncOk = $true
                    if ($Matches[1] -eq '0') { Write-Warn2 "threads 表中未找到该会话记录（可能从未在桌面端打开过）" }
                    else { Write-Ok "桌面端会话注册表已同步：threads.model_provider -> $targetId（$($Matches[1]) 条）" }
                    break
                }
                $lastOut = $r; $lastCode = $code
            }
        } finally {
            $ErrorActionPreference = $eap
            Remove-Item $tmpPy -ErrorAction SilentlyContinue
        }
        if (-not $syncOk) {
            if (-not $lastOut -and $lastCode -eq $null) {
                Write-Err2 "未找到可用的 python，无法同步 sqlite 会话注册表（rollout 已改写；可安装 python 后重跑本命令）"
            } else {
                Write-Err2 "sqlite 同步失败（python 退出码 $lastCode），完整输出如下："
                Write-Host ($lastOut.Trim())
                Write-Info "排查提示：若报 database is locked，请托盘彻底退出 ChatGPT 后重试"
            }
        }
    }
    Write-Warn2 "改写后请彻底退出 ChatGPT 桌面端（托盘退出）再重开，恢复该会话即走新供应商"
    return
}

# 无参数时显示帮助
Write-Host ""
Write-Host "Codex 供应商切换工具" -ForegroundColor Cyan
Write-Host "  -List / -L                列出所有供应商（含内置官方 official）"
Write-Host "  -Show / -S                查看当前配置（模式判定 + 环境变量残留警告）"
Write-Host "  -Probe <id> / -Pr         只探测列出该供应商可用模型，不切换"
Write-Host "  -Provider <id> / -P       切换供应商：自动探测模型并交互选择（选中即回写默认模型）"
Write-Host "                            加 -Model 显式指定模型；加 -Auto 跳过菜单直接用默认模型"
Write-Host "  -P official               切回 OpenAI 官方（ChatGPT 登录走 auth.json，自动清理劫持环境变量与模型 catalog）"
Write-Host "  -Add <id> / -A            添加供应商（需 -BaseUrl -ApiKey，可选 -Model -WireApi）"
Write-Host "  -Remove <id> / -R         删除供应商（official 不可删）"
Write-Host "  -Session <id|路径> / -Se  改写指定会话钉死的供应商为当前供应商（可选 -Model 同步改模型）"
Write-Host "  -Find <关键词> / -F       按关键词搜索会话，拿到 session id 后配合 -Se 使用"
Write-Host ""
Write-Host "模型 catalog：relay 切换时自动生成 ~/.codex/model-providers/catalog-<id>.json 并重指" -ForegroundColor DarkGray
Write-Host "  model_catalog_json，桌面端模型列表/显示名以此为准；-Show 可诊断 catalog 与当前模型是否匹配" -ForegroundColor DarkGray
Write-Host ""
Write-Host "供应商清单文件：$ProvFile（可直接手动编辑）"
