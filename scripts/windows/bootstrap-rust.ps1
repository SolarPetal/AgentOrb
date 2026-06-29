<#
.SYNOPSIS
  Help install Rust toolchain on Windows host for Agent Orb.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

Write-Step 'Checking Rust toolchain'
if (Get-Command cargo -ErrorAction SilentlyContinue) {
  Write-Host "cargo already installed: $(cargo --version)"
  Write-Host "rustc: $(rustc --version)"
  exit 0
}

Write-Step 'Rust is not installed'
if (Get-Command winget -ErrorAction SilentlyContinue) {
  Write-Host 'winget detected. You can install Rust with:'
  Write-Host '  winget install --id Rustlang.Rustup -e'
} else {
  Write-Host 'winget not found. Install Rust from:'
  Write-Host '  https://rustup.rs/'
}

Write-Host "`nIf Rust installer asks for Visual Studio C++ Build Tools, install them, then reopen PowerShell."
Write-Host 'After installation run:'
Write-Host '  .\scripts\windows\install-agent-orb.ps1'
