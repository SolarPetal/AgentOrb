<#
.SYNOPSIS
  Smoke test Agent Orb Windows CLI/daemon.
#>
[CmdletBinding()]
param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'agent-orb\bin')
)

$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Invoke-Status([string]$Token) {
  Invoke-RestMethod -Uri 'http://127.0.0.1:17321/v1/status' -Headers @{ Authorization = "Bearer $Token" }
}

$AgentOrb = Join-Path $InstallDir 'agent_orb.exe'
$AgentOrbd = Join-Path $InstallDir 'agent_orbd.exe'
if (-not (Test-Path $AgentOrb)) { throw "Missing $AgentOrb. Run install-agent-orb.ps1 first." }
if (-not (Test-Path $AgentOrbd)) { throw "Missing $AgentOrbd. Run install-agent-orb.ps1 first." }

$ConfigDir = Join-Path $env:APPDATA 'agent-orb'
$TokenPath = Join-Path $ConfigDir 'token'

Write-Step 'Stopping existing daemon if it is running'
Get-Process agent_orbd -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

Write-Step 'Starting daemon'
$Daemon = Start-Process -FilePath $AgentOrbd -WindowStyle Hidden -PassThru
try {
  $Ready = $false
  for ($i = 0; $i -lt 40; $i++) {
    try {
      $Health = Invoke-RestMethod -Uri 'http://127.0.0.1:17321/health' -TimeoutSec 1
      if ($Health.ok) { $Ready = $true; break }
    } catch {
      Start-Sleep -Milliseconds 250
    }
  }
  if (-not $Ready) { throw 'daemon did not become healthy on 127.0.0.1:17321' }

  if (-not (Test-Path $TokenPath)) { throw "Token file was not created: $TokenPath" }
  $Token = (Get-Content -Raw $TokenPath).Trim()
  if (-not $Token) { throw 'Token file is empty' }

  Write-Step 'Initial status'
  Invoke-Status $Token | ConvertTo-Json -Depth 6

  Write-Step 'Happy path: cmd /C echo hello'
  & $AgentOrb run -- cmd /C echo hello
  if ($LASTEXITCODE -ne 0) { throw "happy path returned exit code $LASTEXITCODE" }
  $Status = Invoke-Status $Token
  $Status | ConvertTo-Json -Depth 6
  if ($Status.status -ne 'completed') { throw "expected completed, got $($Status.status)" }

  Write-Step 'Clear completed status'
  Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:17321/v1/status/clear' -Headers @{ Authorization = "Bearer $Token" } | ConvertTo-Json -Depth 6

  Write-Step 'Failed path: cmd /C exit 3'
  & $AgentOrb run -- cmd /C exit 3
  $ExitCode = $LASTEXITCODE
  Write-Host "wrapper exit code=$ExitCode"
  if ($ExitCode -ne 3) { throw "expected wrapper exit code 3, got $ExitCode" }
  $Status = Invoke-Status $Token
  $Status | ConvertTo-Json -Depth 6
  if ($Status.status -ne 'failed') { throw "expected failed, got $($Status.status)" }

  Write-Step 'Smoke test passed'
} finally {
  if ($Daemon -and -not $Daemon.HasExited) {
    Stop-Process -Id $Daemon.Id -Force -ErrorAction SilentlyContinue
  }
}
