# 가격인하(price-inha) 한 줄 설치 — 윈도(PowerShell 5.1 이상).
#
#   irm https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.ps1 | iex
#
# 선택을 줄 때:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.ps1))) -Version v0.1.0 -NoSetup
#
# 하는 일: 릴리스 묶음(price-inha-<판>-windows-x86_64.zip)과 .sha256 을 https 로 받아 SHA-256 을 확인한 뒤에만
# %LOCALAPPDATA%\price-inha\bin\price-inha.exe 에 놓고, 그 폴더를 사용자 PATH(관리자 권한 없이)에 더하고,
# 끝에 `price-inha setup` 을 실행한다(대화형일 때).
# 하지 않는 일: 관리자 권한 · 소스로 짓기(Rust 가 없다고 본다) · 남의 설치 스크립트 실행 · 비밀값 묻기.
# 근거: docs/dev/decisions.md 결정 24 · 27, docs/research/install-and-cli.md 8.2(원본 Hermes · OpenClaw 의 install.ps1).
# iex 로 돌 때 `exit` 는 창을 닫으므로, 파일로 돌 때만 종료 코드로 끝낸다.
param(
  [string]$Version = "",
  [string]$From = "",
  [string]$Prefix = "",
  [switch]$NoPathUpdate,
  [switch]$NoSetup,
  [switch]$DryRun,
  [switch]$Uninstall,
  [switch]$Help
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# PowerShell 5.1(.NET Framework)은 TLS 1.2 를 켜야 GitHub 에 닿는다.
if ($PSVersionTable.PSEdition -ne 'Core') {
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

$RepoUrl = 'https://github.com/YeongWon2/price-inha'
$TaskName = 'PriceInha'
$ExeName = 'price-inha.exe'
$AsFile = [bool]$PSCommandPath

function Say([string]$Text) { Write-Host $Text }

# 설치를 멈춘다(이 오류만 잡아 글을 보인다).
function Stop-Install([string]$Text) { throw [System.Exception]::new("가격인하 설치: $Text") }

function Show-Usage {
  Say @'
가격인하(price-inha) 설치 — 윈도

쓰는 법: install.ps1 [선택]

  -Version <태그>     그 릴리스를 깐다(예: v0.1.0). 없으면 최신 릴리스
  -From <파일|주소>   이 릴리스 묶음(.zip)을 깐다. 옆에 <같은 이름>.sha256 이 있어야 한다(https 주소만)
  -Prefix <폴더>      놓을 곳(기본 %LOCALAPPDATA%\price-inha\bin)
  -NoPathUpdate       사용자 PATH 를 고치지 않는다(넣는 법만 알린다)
  -NoSetup            끝에 `price-inha setup` 을 실행하지 않는다
  -Uninstall          실행 파일만 지운다(보관 폴더는 그대로 둔다)
  -DryRun             무엇을 할지만 보이고 아무것도 쓰지 않는다
  -Help               이 도움말

환경 변수:
  PRICE_INHA_RELEASE_URL  릴리스 파일이 있는 https 폴더 주소(기본: GitHub 릴리스)
  PRICE_INHA_PREFIX       -Prefix 와 같다
  PRICE_INHA_HOME         보관 폴더(지울 때 알려 주는 위치. 기본 %USERPROFILE%\.price-inha)

필요한 것: Claude Code CLI(구독 로그인) — `price-inha setup` 이 찾고, 없으면 설치 · 로그인 방법을 알려 준다.
git clone · Rust 는 필요 없다(릴리스가 올라와 있을 때).
'@
}

function Assert-Https([string]$Url) {
  if (-not $Url.StartsWith('https://')) { Stop-Install "https 주소만 받는다: $Url" }
}

function Get-NormVersion([string]$Text) {
  if (-not $Text) { Stop-Install '판 이름이 비었다' }
  $v = if ($Text.StartsWith('v')) { $Text } else { "v$Text" }
  if ($v -notmatch '^[0-9A-Za-z._-]+$') { Stop-Install "판 이름이 이상하다: $Text" }
  $v
}

function Get-Target {
  $arch = if ($env:PRICE_INHA_ARCH) { $env:PRICE_INHA_ARCH } else { $env:PROCESSOR_ARCHITECTURE }
  switch -Regex ($arch) {
    '^(AMD64|x86_64)$' { return 'windows-x86_64' }
    '^ARM64$' {
      Say '참고: ARM64 윈도에서는 x86_64 묶음을 에뮬레이션으로 돌린다'
      return 'windows-x86_64'
    }
    default { Stop-Install "지원하지 않는 CPU: $arch — 지원: x86_64(AMD64)" }
  }
}

function Test-Elevated {
  if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') { return $false }
  $id = [Security.Principal.WindowsIdentity]::GetCurrent()
  ([Security.Principal.WindowsPrincipal]$id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# --- 받기 · 확인
function Get-Remote([string]$Url, [string]$OutFile) {
  Assert-Https $Url
  Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile
}

function Get-Sha256([string]$Path) {
  (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

# 묶음의 SHA-256 을 .sha256 파일과 맞춰 본다 — 맞지 않으면 아무것도 깔지 않고 멈춘다.
function Assert-Sha256([string]$Archive, [string]$ShaFile) {
  $first = (Get-Content -LiteralPath $ShaFile -TotalCount 1)
  if (-not $first) { Stop-Install "체크섬 파일이 이상하다: $ShaFile" }
  $parts = $first.Trim() -split '\s+'
  $want = $parts[0].ToLowerInvariant()
  if ($want -notmatch '^[0-9a-f]{64}$') { Stop-Install "체크섬 파일이 이상하다: $ShaFile" }
  if ($parts.Count -ge 2) {
    $named = $parts[1].TrimStart('*')
    if ($named -ne (Split-Path -Leaf $Archive)) { Stop-Install "체크섬 파일이 다른 묶음의 것이다: $named" }
  }
  $got = Get-Sha256 $Archive
  if ($want -ne $got) { Stop-Install "SHA-256 이 맞지 않는다 — 설치하지 않는다 (기대 $want, 받은 것 $got)" }
  Say "SHA-256 확인: $(Split-Path -Leaf $Archive)"
}

# --- 풀기 · 점검 · 놓기
function Expand-Release([string]$Archive, [string]$Into) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $zip = [System.IO.Compression.ZipFile]::OpenRead($Archive)
  try {
    foreach ($entry in $zip.Entries) {
      $name = $entry.FullName -replace '\\', '/'
      if ($name.StartsWith('/') -or $name -match '^[A-Za-z]:' -or $name -match '(^|/)\.\.(/|$)') {
        Stop-Install '묶음 안에 이상한 경로가 있다 — 설치하지 않는다'
      }
    }
  } finally { $zip.Dispose() }
  [System.IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Into)
  $found = Get-ChildItem -LiteralPath $Into -Recurse -Depth 1 -File -Filter $ExeName | Select-Object -First 1
  if (-not $found) { Stop-Install "묶음 안에 $ExeName 이 없다" }
  $found.FullName
}

function Assert-Binary([string]$Exe, [string]$Want) {
  $out = $null
  try { $out = (& $Exe --version 2>$null | Select-Object -First 1) } catch { $out = $null }
  if (-not $out) { Stop-Install '받은 실행 파일이 이 컴퓨터에서 돌지 않는다' }
  if (-not "$out".StartsWith('price-inha ')) { Stop-Install '받은 실행 파일이 price-inha 가 아니다' }
  if ($Want -and "$out" -ne "price-inha $($Want.TrimStart('v'))") {
    Stop-Install "판이 다르다: 기대 $($Want.TrimStart('v')), 받은 것 $("$out".Substring(11))"
  }
}

# 같은 폴더의 임시 이름으로 복사한 뒤 이름을 바꾼다. 앞 실행 파일은 .prev 로 남긴다
# (도는 실행 파일도 이름은 바꿀 수 있다).
function Install-Binary([string]$Exe, [string]$Dest) {
  $dir = Split-Path -Parent $Dest
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $new = Join-Path $dir ".price-inha.new.$PID.exe"
  Copy-Item -LiteralPath $Exe -Destination $new -Force
  if (Test-Path -LiteralPath $Dest) {
    $prev = "$Dest.prev"
    if (Test-Path -LiteralPath $prev) { Remove-Item -LiteralPath $prev -Force }
    Move-Item -LiteralPath $Dest -Destination $prev
  }
  Move-Item -LiteralPath $new -Destination $Dest
  Say "놓았다: $Dest ($(& $Dest --version))"
}

# --- 사용자 PATH(관리자 권한 없이 — 사용자 환경 변수)
function Test-OnPath([string]$List, [string]$Dir) {
  $want = $Dir.TrimEnd('\').ToLowerInvariant()
  foreach ($item in ($List -split ';')) {
    if ($item.TrimEnd('\').ToLowerInvariant() -eq $want) { return $true }
  }
  $false
}

function Add-UserPath([string]$Dir) {
  $user = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (Test-OnPath "$user" $Dir) { Say "사용자 PATH 에 이미 있다: $Dir"; return }
  if ($NoPathUpdate) {
    Say "$Dir 가 PATH 에 없다. 쓰려면 사용자 환경 변수 Path 에 이 폴더를 더한다(-NoPathUpdate 라 고치지 않았다)"
    return
  }
  $next = if ($user) { "$($user.TrimEnd(';'));$Dir" } else { $Dir }
  [Environment]::SetEnvironmentVariable('Path', $next, 'User')
  if (-not (Test-OnPath "$env:Path" $Dir)) { $env:Path = "$env:Path;$Dir" }
  Say "사용자 PATH 에 더했다: $Dir (새 창에서도 price-inha 로 부를 수 있다)"
}

function Remove-UserPath([string]$Dir) {
  $user = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not (Test-OnPath "$user" $Dir)) { return }
  $want = $Dir.TrimEnd('\').ToLowerInvariant()
  $kept = ($user -split ';') | Where-Object { $_ -and $_.TrimEnd('\').ToLowerInvariant() -ne $want }
  [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
  Say "사용자 PATH 에서 뺐다: $Dir"
}

# --- 지우기: 실행 파일만. 보관 폴더는 건드리지 않는다.
function Invoke-Uninstall([string]$Dest, [string]$Dir) {
  if (Get-Command schtasks.exe -ErrorAction SilentlyContinue) {
    & schtasks.exe /Query /TN $TaskName *> $null
    if ($LASTEXITCODE -eq 0) {
      Stop-Install "자동 실행 등록(예약 작업 $TaskName)이 남아 있다. 먼저 ``price-inha service uninstall`` 을 돌린 뒤 다시 지운다"
    }
  }
  $found = $false
  foreach ($f in @($Dest, "$Dest.prev")) {
    if (Test-Path -LiteralPath $f) {
      $found = $true
      if ($DryRun) { Say "[dry-run] 지운다: $f" } else { Remove-Item -LiteralPath $f -Force; Say "지웠다: $f" }
    }
  }
  if (-not $found) { Say "깔린 실행 파일이 없다: $Dest" }
  if (-not $DryRun -and -not $NoPathUpdate) { Remove-UserPath $Dir }
  $data = if ($env:PRICE_INHA_HOME) { $env:PRICE_INHA_HOME } else { Join-Path $env:USERPROFILE '.price-inha' }
  Say "보관 폴더는 그대로 두었다: $data"
  Say '  (키 · 기록 · 모의계좌가 들어 있다. 자료까지 지우려면 직접 지운다 — 되돌릴 수 없다)'
}

# --- 릴리스 받기(소스로 짓지 않는다 — 윈도에는 Rust 가 없다고 본다)
function Get-Release([string]$Tmp, [string]$Target) {
  $base = if ($env:PRICE_INHA_RELEASE_URL) { $env:PRICE_INHA_RELEASE_URL.TrimEnd('/') }
          elseif ($script:Wanted) { "$RepoUrl/releases/download/$script:Wanted" }
          else { "$RepoUrl/releases/latest/download" }
  Assert-Https $base
  try {
    if (-not $script:Wanted) {
      Get-Remote "$base/LATEST" (Join-Path $Tmp 'LATEST')
      $script:Wanted = Get-NormVersion ((Get-Content -LiteralPath (Join-Path $Tmp 'LATEST') -TotalCount 1).Trim())
    }
    $name = "price-inha-$script:Wanted-$Target.zip"
    Get-Remote "$base/$name" (Join-Path $Tmp $name)
  } catch {
    if ($_.Exception.Message.StartsWith('가격인하 설치:')) { throw }
    Say "아직 올라온 릴리스가 없습니다(이 컴퓨터 $Target 에 맞는 묶음을 받지 못했다)."
    Say "  릴리스: $RepoUrl/releases"
    Say '  윈도에서는 소스로 짓지 않는다(Rust 가 필요 없게 하려고). 릴리스가 올라온 뒤 다시 돌린다.'
    Stop-Install '릴리스 없음'
  }
  try { Get-Remote "$base/$name.sha256" (Join-Path $Tmp "$name.sha256") }
  catch { Stop-Install "체크섬 파일을 받지 못했다: $base/$name.sha256 — 확인 없이는 깔지 않는다" }
  Join-Path $Tmp $name
}

function Get-FromArchive([string]$Tmp) {
  $name = Split-Path -Leaf $From
  $archive = Join-Path $Tmp $name
  if ($From -match '^[A-Za-z][A-Za-z0-9+.-]*://') {
    Assert-Https $From
    try { Get-Remote $From $archive } catch { Stop-Install "받지 못했다: $From" }
    try { Get-Remote "$From.sha256" "$archive.sha256" }
    catch { Stop-Install "체크섬 파일을 받지 못했다: $From.sha256 — 확인 없이는 깔지 않는다" }
  } else {
    if (-not (Test-Path -LiteralPath $From -PathType Leaf)) { Stop-Install "파일이 없다: $From" }
    if (-not (Test-Path -LiteralPath "$From.sha256" -PathType Leaf)) {
      Stop-Install "체크섬 파일이 없다: $From.sha256 — 확인 없이는 깔지 않는다"
    }
    Copy-Item -LiteralPath $From -Destination $archive
    Copy-Item -LiteralPath "$From.sha256" -Destination "$archive.sha256"
  }
  $archive
}

function Show-Plan([string]$Target, [string]$Dest, [string]$Dir) {
  Say "[dry-run] 대상: $Target"
  if ($From) { Say "[dry-run] 묶음: $From 와 $From.sha256" }
  else {
    $where = if ($env:PRICE_INHA_RELEASE_URL) { $env:PRICE_INHA_RELEASE_URL } else { "$RepoUrl/releases" }
    $ver = if ($Version) { $Version } else { '<최신>' }
    Say "[dry-run] 릴리스: $where 의 price-inha-$ver-$Target.zip 와 .sha256 (https 만). 없으면 소스로 짓지 않고 끝낸다"
  }
  Say '[dry-run] SHA-256 확인 — 맞지 않으면 설치하지 않는다'
  Say "[dry-run] 놓기: $Dest (관리자 권한 없음)"
  if ($NoPathUpdate) { Say '[dry-run] 사용자 PATH 는 고치지 않는다(-NoPathUpdate)' }
  else { Say "[dry-run] 사용자 PATH 에 $Dir 를 더한다(이미 있으면 그대로)" }
  if (-not $NoSetup) { Say '[dry-run] 끝에 `price-inha setup` 을 실행할지 묻는다(대화형일 때)' }
}

function Invoke-Setup([string]$Dest) {
  $interactive = [Environment]::UserInteractive -and -not [Console]::IsInputRedirected
  if ($NoSetup -or -not $interactive) {
    Say '다음: `price-inha setup` 을 실행하세요 (AI 연결 · 키 · 종목 · 메신저를 묻습니다).'
    Say '      화면은 `price-inha open`, 키 없이 먼저 보려면 `price-inha run --demo`.'
    return
  }
  $answer = Read-Host '`price-inha setup`을 지금 실행할까요? [Y/n]'
  if ($answer -match '^(|y|Y|yes|YES|예|네|ㅇ)$') { & $Dest setup }
  else { Say '다음: `price-inha setup` 을 실행하세요.' }
}

function Invoke-Main {
  if ($Help) { Show-Usage; return }
  $dir = if ($Prefix) { $Prefix } elseif ($env:PRICE_INHA_PREFIX) { $env:PRICE_INHA_PREFIX }
         else { Join-Path $env:LOCALAPPDATA 'price-inha\bin' }
  if (-not $dir) { Stop-Install '-Prefix 가 비었다(LOCALAPPDATA 가 없다)' }
  $dest = Join-Path $dir $ExeName
  if ($Uninstall) { Invoke-Uninstall $dest $dir; return }
  if (-not $DryRun -and (Test-Elevated)) {
    Stop-Install '관리자 권한으로 돌리지 않는다 — 보통 PowerShell 창에서 다시 돌린다(사용자 폴더에 깐다)'
  }
  $target = Get-Target
  if ($Version) { $script:Wanted = Get-NormVersion $Version }
  if ($From -and $From -match '^[A-Za-z][A-Za-z0-9+.-]*://') { Assert-Https $From }
  if ($env:PRICE_INHA_RELEASE_URL) { Assert-Https $env:PRICE_INHA_RELEASE_URL }
  if ($DryRun) { Show-Plan $target $dest $dir; return }
  $tmp = Join-Path ([IO.Path]::GetTempPath()) "price-inha-install.$PID.$([Guid]::NewGuid().ToString('N'))"
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    $archive = if ($From) { Get-FromArchive $tmp } else { Get-Release $tmp $target }
    Assert-Sha256 $archive "$archive.sha256"
    $exe = Expand-Release $archive (Join-Path $tmp 'x')
    Assert-Binary $exe $script:Wanted
    Install-Binary $exe $dest
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
  Add-UserPath $dir
  Say ''
  Invoke-Setup $dest
}

$script:Wanted = ''
try {
  Invoke-Main
  if ($AsFile) { exit 0 }
} catch {
  $message = $_.Exception.Message
  [Console]::Error.WriteLine($message)
  if ($AsFile) { exit 1 }
  $global:LASTEXITCODE = 1
}
