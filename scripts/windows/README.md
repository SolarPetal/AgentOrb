# Agent Orb Windows host smoke test

Open **Windows PowerShell** in the AgentOrb repo and run:

```powershell
Set-ExecutionPolicy -Scope Process Bypass -Force
.\scripts\windows\bootstrap-rust.ps1
.\scripts\windows\install-agent-orb.ps1
.\scripts\windows\smoke-agent-orb.ps1
```

Optional adapter shims:

```powershell
.\scripts\windows\install-agent-orb.ps1 -CreateAdapterShims
codex-orb
claude-orb
```

Requirements:

- Rust for Windows: <https://rustup.rs/>
- MSVC C++ Build Tools if Rust asks for a linker
- Node/Tauri only needed for Windows UI builds; CLI/daemon smoke does not require Node
