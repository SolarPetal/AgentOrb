<#
.SYNOPSIS
  Build and install Agent Orb CLI/daemon on Windows host.

.DESCRIPTION
  Run this from Windows PowerShell inside the AgentOrb repo.
  It builds Rust workspace binaries and installs:
    agent_orb.exe
    agent_orbd.exe
  into:
    $env:LOCALAPPDATA\agent-orb\bin

  It does not replace codex.exe or claude.exe.
#>
[CmdletBinding()]
param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'agent-orb\bin'),
  [switch]$CreateAdapterShims
)

$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Require-Command([string]$Name, [string]$InstallHint) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "Missing required command '$Name'. $InstallHint"
  }
}

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $RepoRoot

Write-Step 'Checking required tools'
Require-Command cargo 'Install Rust from https://rustup.rs/ then reopen PowerShell.'
Require-Command rustc 'Install Rust from https://rustup.rs/ then reopen PowerShell.'
Write-Host "cargo: $(cargo --version)"
Write-Host "rustc: $(rustc --version)"

Write-Step 'Building Rust workspace release binaries'
cargo build --release -p agent-orb-cli -p agent-orb-daemon

$ReleaseDir = Join-Path $RepoRoot 'target\release'
$AgentOrb = Join-Path $ReleaseDir 'agent_orb.exe'
$AgentOrbd = Join-Path $ReleaseDir 'agent_orbd.exe'
if (-not (Test-Path $AgentOrb)) { throw "Missing build output: $AgentOrb" }
if (-not (Test-Path $AgentOrbd)) { throw "Missing build output: $AgentOrbd" }

Write-Step "Installing binaries to $InstallDir"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force $AgentOrb $InstallDir
Copy-Item -Force $AgentOrbd $InstallDir

if ($CreateAdapterShims) {
  Write-Step 'Creating optional adapter shims'
  @'
@echo off
"%~dp0agent_orb.exe" run -- codex %*
'@ | Set-Content -NoNewline -Encoding ASCII (Join-Path $InstallDir 'codex-orb.cmd')
  @'
@echo off
"%~dp0agent_orb.exe" run -- claude %*
'@ | Set-Content -NoNewline -Encoding ASCII (Join-Path $InstallDir 'claude-orb.cmd')
}

Write-Step 'Ensuring install dir is on user PATH'
$UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$PathParts = @()
if ($UserPath) { $PathParts = $UserPath -split ';' | Where-Object { $_ } }
if ($PathParts -notcontains $InstallDir) {
  $NewPath = (($PathParts + $InstallDir) -join ';')
  [Environment]::SetEnvironmentVariable('Path', $NewPath, 'User')
  Write-Host "Added to user PATH. Open a new PowerShell window to use agent_orb globally."
} else {
  Write-Host 'Install dir already present in user PATH.'
}

Write-Step 'Installation summary'
Write-Host "agent_orb:  $(Join-Path $InstallDir 'agent_orb.exe')"
Write-Host "agent_orbd: $(Join-Path $InstallDir 'agent_orbd.exe')"
if ($CreateAdapterShims) {
  Write-Host "codex-orb:  $(Join-Path $InstallDir 'codex-orb.cmd')"
  Write-Host "claude-orb: $(Join-Path $InstallDir 'claude-orb.cmd')"
}

Write-Host "`nNext smoke test:"
Write-Host "  powershell -ExecutionPolicy Bypass -File scripts\windows\smoke-agent-orb.ps1 -InstallDir '$InstallDir'"
