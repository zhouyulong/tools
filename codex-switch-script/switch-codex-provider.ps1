# switch-codex-provider.ps1 - Codex 多供应商/模型 一键切换脚本
# 原理：改写 ~/.codex/config.toml 的根键 model_provider / model，并同步环境变量 CODEX_API_KEY
#
# 用法示例（支持短参数：-L=-List -S=-Show -P=-Provider -M=-Model -A=-Add -R=-Remove -Se=-Session）：
#   .\switch-codex-provider.ps1 -List                          # 列出所有已配置供应商
#   .\switch-codex-provider.ps1 -Provider sharedchat           # 切换到 sharedchat（用其默认模型）
#   .\switch-codex-provider.ps1 -P sharedchat -M gpt-5.6-sol   # 短参数等价写法
#   .\switch-codex-provider.ps1 -Show                          # 查看当前生效配置
#   添加新供应商：
#   .\switch-codex-provider.ps1 -Add myrelay -BaseUrl "https://xxx.com/v1" -ApiKey "sk-xxx" -Model "gpt-5.6-sol" -WireApi responses
#   删除供应商：
#   .\switch-codex-provider.ps1 -Remove myrelay
#   把指定会话（旧对话）钉死的供应商改为当前供应商，使其恢复/继续时走新供应商：
#   .\switch-codex-provider.ps1 -Session <rollout文件路径 或 session id 或 文件名关键字> [-Provider id] [-Model 模型名]
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
    [string]$Session
)

$ErrorActionPreference = "Stop"
$CodexDir    = "$env:USERPROFILE\.codex"
$CodexConfig = "$CodexDir\config.toml"
$ProvFile    = "$CodexDir\providers.json"
if ($PSScriptRoot -and (Test-Path "$PSScriptRoot\providers.json")) {
    $ProvFile = "$PSScriptRoot\providers.json"
}

function Write-Info($m)    { Write-Host "[信息] " -ForegroundColor Blue -NoNewline;   Write-Host $m }
function Write-Ok($m)      { Write-Host "[成功] " -ForegroundColor Green -NoNewline;  Write-Host $m }
function Write-Warn2($m)   { Write-Host "[警告] " -ForegroundColor Yellow -NoNewline; Write-Host $m }
function Write-Err2($m)    { Write-Host "[错误] " -ForegroundColor Red -NoNewline;    Write-Host $m }

function Get-Providers {
    if (-not (Test-Path $ProvFile)) {
        $seed = [ordered]@{
            sharedchat = [ordered]@{
                name = "sharedchat"; base_url = "https://new.sharedchat.cc/codex"
                wire_api = "responses"; default_model = "gpt-5.6-sol"; api_key = ""
            }
        }
        $seed | ConvertTo-Json -Depth 5 | Set-Content -Path $ProvFile -Encoding UTF8
        Write-Info "已生成供应商清单：$ProvFile"
    }
    return (Get-Content $ProvFile -Raw -Encoding UTF8 | ConvertFrom-Json)
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

function Backup-Config {
    if (Test-Path $CodexConfig) {
        $bak = "$CodexConfig.backup.switch_$(Get-Date -Format 'yyyyMMdd_HHmmss')"
        Copy-Item $CodexConfig $bak
        Write-Info "已备份配置到：$bak"
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
    Write-Host ""
    Write-Host "已配置的 Codex 供应商（* 为当前生效）：" -ForegroundColor Cyan
    foreach ($prop in $providers.PSObject.Properties) {
        $p = $prop.Value
        $mark = if ($prop.Name -eq $current) { "*" } else { " " }
        $keyMask = if ($p.api_key) { $p.api_key.Substring(0, [Math]::Min(8, $p.api_key.Length)) + "..." } else { "(未设置)" }
        Write-Host ("$mark {0,-14} {1}  协议={2}  默认模型={3}  Key={4}" -f `
            $prop.Name, $p.base_url, $p.wire_api, $p.default_model, $keyMask)
    }
    Write-Host ""
    return
}

if ($Show) {
    Write-Host ""
    Write-Info "当前 config.toml 关键配置："
    if (Test-Path $CodexConfig) {
        Get-Content $CodexConfig -Encoding UTF8 | Where-Object {
            $_ -match '^\s*(model|model_provider|model_reasoning_effort)\s*=' -or $_ -match '^\[model_providers\.'
        } | ForEach-Object { Write-Host "  $_" }
    } else { Write-Warn2 "未找到 $CodexConfig" }
    $envKey = [Environment]::GetEnvironmentVariable("CODEX_API_KEY", "User")
    if ($envKey) { Write-Info ("CODEX_API_KEY(User) = " + $envKey.Substring(0, [Math]::Min(8, $envKey.Length)) + "...") }
    else { Write-Warn2 "用户级环境变量 CODEX_API_KEY 未设置" }
    Write-Host ""
    return
}

if ($Remove) {
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
    $providers = Get-Providers
    $prop = $providers.PSObject.Properties[$Provider]
    if (-not $prop) {
        Write-Err2 "供应商 '$Provider' 不存在。用 -List 查看已有供应商，或用 -Add 添加。"
        return
    }
    $p = $prop.Value
    $targetModel = if ($Model) { $Model } elseif ($p.default_model) { $p.default_model } else { "" }
    if (-not $targetModel) { Write-Err2 "未指定模型：请用 -Model 传入，或在 providers.json 中为该供应商设置 default_model"; return }

    Backup-Config

    # 1) 写供应商 section
    $lines = @()
    if (Test-Path $CodexConfig) { $lines = Get-Content $CodexConfig -Encoding UTF8 }
    $providerSettings = [ordered]@{
        name = ConvertTo-TomlString $p.name
        base_url = ConvertTo-TomlString $p.base_url
        wire_api = ConvertTo-TomlString $p.wire_api
        env_key = ConvertTo-TomlString "CODEX_API_KEY"
        supports_websockets = "false"
    }
    $lines = Set-TomlSectionValues -Lines $lines -SectionName "model_providers.$Provider" -Settings $providerSettings

    # 2) 改根 section 选择器：model_provider + model
    $rootSettings = [ordered]@{
        model_provider = ConvertTo-TomlString $Provider
        model = ConvertTo-TomlString $targetModel
    }
    $lines = Set-TomlSectionValues -Lines $lines -SectionName "" -Settings $rootSettings
    # 无 BOM 写入：Codex(Rust) 的 TOML 解析器不接受 BOM，PS5.1 的 Set-Content -Encoding UTF8 会写 BOM
    [System.IO.File]::WriteAllLines($CodexConfig, [string[]]$lines, [System.Text.UTF8Encoding]::new($false))
    Write-Ok "config.toml 已更新：model_provider=$Provider, model=$targetModel"

    # 3) 同步环境变量（用户级持久 + 当前会话即时）
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

    Write-Host ""
    Write-Ok "切换完成：$Provider ($($p.base_url)) 模型=$targetModel"
    Write-Warn2 "注意：需重启终端 / ChatGPT 桌面端后新配置才会生效"
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

    # 解析会话文件：直接路径 / session id / 文件名关键字
    $files = @()
    if (Test-Path $Session) {
        $files += (Resolve-Path $Session).Path
    } else {
        $files = Get-ChildItem "$CodexDir\sessions", "$CodexDir\archived_sessions" -Recurse -Filter "*.jsonl" -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "*$Session*" } | Select-Object -ExpandProperty FullName
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
        Write-Warn2 "会话中没有需要改写的供应商记录（或已是指定供应商），文件未修改"
        Remove-Item "$path.bak-switch-$stamp" -ErrorAction SilentlyContinue
        return
    }

    [System.IO.File]::WriteAllLines($path, [string[]]$out.ToArray(), [System.Text.UTF8Encoding]::new($false))
    Write-Ok "会话供应商已改写：$($oldIds -join ',') -> $targetId$(if ($Model) { "，模型 -> $Model" })"
    Write-Info "文件：$path"
    Write-Info "已备份：$path.bak-switch-$stamp"
    Write-Warn2 "改写前请彻底退出 ChatGPT 桌面端（托盘退出），重开后恢复该会话即走新供应商"
    return
}

# 无参数时显示帮助
Write-Host ""
Write-Host "Codex 供应商切换工具" -ForegroundColor Cyan
Write-Host "  -List / -L                列出所有供应商"
Write-Host "  -Show / -S                查看当前配置"
Write-Host "  -Provider <id> / -P       切换到指定供应商（可加 -Model 临时指定模型）"
Write-Host "  -Add <id> / -A            添加供应商（需 -BaseUrl -ApiKey，可选 -Model -WireApi）"
Write-Host "  -Remove <id> / -R         删除供应商"
Write-Host "  -Session <id|路径> / -Se  改写指定会话钉死的供应商为当前供应商（可选 -Model 同步改模型）"
Write-Host ""
Write-Host "供应商清单文件：$ProvFile（可直接手动编辑）"
