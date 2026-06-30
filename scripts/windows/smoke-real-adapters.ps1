<#
.SYNOPSIS
  Smoke test Agent Orb Windows runtime against real Codex / Claude adapters.

.DESCRIPTION
  Run this from Windows PowerShell after installing Agent Orb on the Windows host:

    .\scripts\windows\install-agent-orb.ps1 -CreateAdapterShims
    .\scripts\windows\smoke-real-adapters.ps1

  The script uses an isolated temporary Agent Orb config directory and a random
  daemon port by default, so it does not overwrite the user's normal
  %APPDATA%\agent-orb config.
#>
[CmdletBinding()]
param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'agent-orb\bin'),
  [string]$ConfigDir = '',
  [int]$Port = 0,
  [switch]$RequireAdapters,
  [switch]$KeepTemp,
  [switch]$NoCreateMissingShims
)

$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Write-Warn([string]$Message) {
  Write-Host "WARN: $Message" -ForegroundColor Yellow
}

function Require-File([string]$Path, [string]$Hint) {
  if (-not (Test-Path $Path)) {
    throw "Missing required file: $Path. $Hint"
  }
}

function New-SmokeConfigDir {
  Join-Path ([System.IO.Path]::GetTempPath()) "agent-orb-real-adapter-$([Guid]::NewGuid().ToString('N'))"
}

function New-SmokePort {
  for ($i = 0; $i -lt 40; $i++) {
    $Candidate = Get-Random -Minimum 34000 -Maximum 45000
    if (Test-PortAvailable $Candidate) { return $Candidate }
  }
  throw 'Could not find an available local TCP port for smoke test.'
}

function Test-PortAvailable([int]$LocalPort) {
  if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
    $Existing = Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort $LocalPort -State Listen -ErrorAction SilentlyContinue
    return $null -eq $Existing
  }

  $Pattern = "127.0.0.1:$LocalPort"
  $Lines = netstat -ano -p tcp | Select-String -SimpleMatch $Pattern
  return $null -eq $Lines
}

function Stop-AgentOrbDaemonOnPort([int]$LocalPort) {
  $ProcessIds = @()

  if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
    $ProcessIds = Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort $LocalPort -State Listen -ErrorAction SilentlyContinue |
      Select-Object -ExpandProperty OwningProcess -Unique
  } else {
    $Pattern = "127.0.0.1:$LocalPort"
    $ProcessIds = netstat -ano -p tcp |
      Select-String -SimpleMatch $Pattern |
      ForEach-Object {
        $Parts = ($_.Line -split '\s+') | Where-Object { $_ }
        $Parts[-1]
      } |
      Select-Object -Unique
  }

  foreach ($OwnerProcessId in $ProcessIds) {
    if (-not $OwnerProcessId) { continue }
    $Process = Get-Process -Id $OwnerProcessId -ErrorAction SilentlyContinue
    if ($Process -and $Process.ProcessName -eq 'agent_orbd') {
      Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    }
  }
}

function Write-SmokeConfig([string]$TargetDir, [int]$DaemonPort) {
  New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null

  $Config = @"
[daemon]
host = "127.0.0.1"
port = $DaemonPort
auto_start = true

[behavior]
silent_threshold_seconds = 20
stuck_threshold_seconds = 180
completed_hold_seconds = 10
error_requires_click_to_clear = true

[privacy]
include_output_sample = false
max_sample_chars = 512
"@

  Set-Content -Path (Join-Path $TargetDir 'config.toml') -Value $Config -Encoding UTF8
}

function Get-Token([string]$TargetDir) {
  $TokenPath = Join-Path $TargetDir 'token'
  if (-not (Test-Path $TokenPath)) {
    throw "Token file was not created: $TokenPath"
  }

  $Token = (Get-Content -Raw $TokenPath).Trim()
  if (-not $Token) {
    throw "Token file is empty: $TokenPath"
  }

  $Token
}

function Invoke-AgentOrbStatus([int]$DaemonPort, [string]$Token) {
  Invoke-RestMethod -Uri "http://127.0.0.1:$DaemonPort/v1/status" -Headers @{ Authorization = "Bearer $Token" }
}

function Assert-AgentOrbStatus([string]$ExpectedSource, [string]$ExpectedStatus, [string]$TargetDir, [int]$DaemonPort) {
  $Token = Get-Token $TargetDir
  $Status = Invoke-AgentOrbStatus $DaemonPort $Token
  $Status | ConvertTo-Json -Depth 6

  if ($Status.source -ne $ExpectedSource -or $Status.status -ne $ExpectedStatus) {
    throw "Expected daemon status $ExpectedSource/$ExpectedStatus, got $($Status.source)/$($Status.status)."
  }

  Write-Host "OK daemon status: $($Status.source)/$($Status.status)" -ForegroundColor Green
}

function Ensure-AdapterShim([string]$Adapter, [string]$TargetInstallDir) {
  $Shim = Join-Path $TargetInstallDir "${Adapter}-orb.cmd"
  if (Test-Path $Shim) { return $Shim }

  if ($NoCreateMissingShims) {
    return $null
  }

  Write-Warn "${Adapter}-orb.cmd was not found; creating a local shim in $TargetInstallDir."
  $ShimContent = @"
@echo off
"%~dp0agent_orb.exe" run -- $Adapter %*
"@
  Set-Content -Path $Shim -Value $ShimContent -NoNewline -Encoding ASCII
  $Shim
}

function Invoke-AdapterRun([string]$Label, [scriptblock]$Command) {
  Write-Step $Label
  & $Command
  $ExitCode = $LASTEXITCODE
  if ($ExitCode -ne 0) {
    throw "$Label failed with exit code $ExitCode"
  }
}

$OwnConfigDir = $false
if (-not $ConfigDir) {
  $ConfigDir = New-SmokeConfigDir
  $OwnConfigDir = $true
}

if ($Port -eq 0) {
  $Port = New-SmokePort
} elseif (-not (Test-PortAvailable $Port)) {
  throw "Port $Port is already in use. Choose another port with -Port."
}

$DetectedAdapters = @()
foreach ($Adapter in @('codex', 'claude')) {
  $Command = Get-Command $Adapter -ErrorAction SilentlyContinue
  if ($Command) {
    $DetectedAdapters += [PSCustomObject]@{
      Name = $Adapter
      Path = $Command.Source
    }
  }
}

if ($DetectedAdapters.Count -eq 0) {
  Write-Warn 'No real Codex or Claude CLI found on PATH.'
  if ($RequireAdapters) {
    throw 'No real adapters detected and -RequireAdapters was set.'
  }
  exit 0
}

$AgentOrb = Join-Path $InstallDir 'agent_orb.exe'
$AgentOrbd = Join-Path $InstallDir 'agent_orbd.exe'
Require-File $AgentOrb 'Run .\scripts\windows\install-agent-orb.ps1 first.'
Require-File $AgentOrbd 'Run .\scripts\windows\install-agent-orb.ps1 first.'

$PreviousConfigDir = $env:AGENT_ORB_CONFIG_DIR
$PreviousDaemonPort = $env:AGENT_ORB_DAEMON_PORT
$HadConfigDir = Test-Path Env:\AGENT_ORB_CONFIG_DIR
$HadDaemonPort = Test-Path Env:\AGENT_ORB_DAEMON_PORT

try {
  Write-Step "Preparing isolated config at $ConfigDir"
  Write-SmokeConfig $ConfigDir $Port
  $env:AGENT_ORB_CONFIG_DIR = $ConfigDir
  $env:AGENT_ORB_DAEMON_PORT = "$Port"

  Write-Step 'Detected real adapters'
  foreach ($Adapter in $DetectedAdapters) {
    Write-Host "  OK $($Adapter.Name): $($Adapter.Path)"
  }

  foreach ($Adapter in $DetectedAdapters) {
    $Name = $Adapter.Name
    $Shim = Ensure-AdapterShim $Name $InstallDir

    Invoke-AdapterRun "agent_orb run -- $Name --version" {
      & $AgentOrb run -- $Name --version
    }
    Assert-AgentOrbStatus $Name 'completed' $ConfigDir $Port

    if ($Shim) {
      Invoke-AdapterRun "${Name}-orb.cmd --version" {
        & $Shim --version
      }
      Assert-AgentOrbStatus $Name 'completed' $ConfigDir $Port
    } else {
      Write-Warn "${Name}-orb.cmd was not created; rerun install-agent-orb.ps1 with -CreateAdapterShims or omit -NoCreateMissingShims."
      if ($RequireAdapters) {
        throw "${Name}-orb.cmd is missing."
      }
    }
  }

  Write-Step 'Real adapter smoke passed'
} finally {
  Stop-AgentOrbDaemonOnPort $Port

  if (-not $HadConfigDir) {
    Remove-Item Env:\AGENT_ORB_CONFIG_DIR -ErrorAction SilentlyContinue
  } else {
    $env:AGENT_ORB_CONFIG_DIR = $PreviousConfigDir
  }

  if (-not $HadDaemonPort) {
    Remove-Item Env:\AGENT_ORB_DAEMON_PORT -ErrorAction SilentlyContinue
  } else {
    $env:AGENT_ORB_DAEMON_PORT = $PreviousDaemonPort
  }

  if ($OwnConfigDir -and -not $KeepTemp) {
    Remove-Item -Recurse -Force $ConfigDir -ErrorAction SilentlyContinue
  } elseif ($KeepTemp) {
    Write-Host "Keeping smoke config: $ConfigDir"
  }
}
