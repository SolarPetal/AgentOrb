use std::{
    env,
    io::{self, Read},
};

use agent_orb_core::{config::Config, event::EventType, source::Source};
use serde_json::{json, Value};

use crate::{
    config::{default_config_dir, ensure_loopback_host, read_token},
    daemon::DaemonClient,
    error::AppError,
    event::build_event,
    source::detect_source,
};

/// Environment variable the wrapper injects into the observed adapter process so
/// hook invocations report against the same Agent Orb session.
pub const SESSION_ENV: &str = "AGENT_ORB_SESSION";

/// Map a Claude Code hook event name to the adapter status string understood by
/// the daemon state machine (`status.hint` payload). Returning `None` means the
/// event carries no orb-relevant state change and should be ignored.
///
/// Reference: https://code.claude.com/docs/en/hooks
pub fn status_for_hook_event(hook_event_name: &str) -> Option<&'static str> {
    match hook_event_name {
        // A new prompt was submitted: Claude starts reasoning.
        "UserPromptSubmit" => Some("thinking"),
        // A tool is about to run: active execution.
        "PreToolUse" => Some("executing"),
        // Tool finished (success or failure): back to reasoning about the result.
        "PostToolUse" | "PostToolUseFailure" | "PostToolBatch" => Some("thinking"),
        // Claude needs the user: permission dialog or idle attention prompt.
        "Notification" | "PermissionRequest" | "Elicitation" => Some("waiting"),
        // Context compaction.
        "PreCompact" => Some("compacting"),
        "PostCompact" => Some("thinking"),
        // Claude finished responding: the turn completed.
        "Stop" => Some("completed"),
        // Session boundaries: fall back to idle.
        "SessionStart" => Some("idle"),
        _ => None,
    }
}

/// Run the `agent_orb hook` subcommand. Reads the hook event JSON from stdin,
/// maps it to a status hint, and posts it to the local daemon for the current
/// session. This never fails in a way that disrupts the host CLI: any missing
/// prerequisite or transport error resolves to `Ok(0)`.
pub async fn run_hook(source_override: Option<String>) -> Result<i32, AppError> {
    // Only report when running inside an Agent Orb wrapped session.
    let Ok(session_id) = env::var(SESSION_ENV) else {
        return Ok(0);
    };
    if session_id.trim().is_empty() {
        return Ok(0);
    }

    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return Ok(0);
    }

    let payload: Value = match serde_json::from_str(input.trim()) {
        Ok(value) => value,
        Err(_) => return Ok(0),
    };

    let hook_event_name = payload
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(status) = status_for_hook_event(hook_event_name) else {
        return Ok(0);
    };

    let source = source_override
        .as_deref()
        .map(detect_source)
        .unwrap_or(Source::Claude);

    let workspace = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| env::current_dir().ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| ".".to_string());

    // Best-effort delivery. Swallow every error so a hook never breaks Claude.
    if post_status(&session_id, source, workspace, status)
        .await
        .is_err()
    {
        return Ok(0);
    }

    Ok(0)
}

async fn post_status(
    session_id: &str,
    source: Source,
    workspace: String,
    status: &str,
) -> Result<(), AppError> {
    let config_dir = default_config_dir();
    let config = Config::load_from_dir_or_default(&config_dir);
    ensure_loopback_host(&config.daemon.host)?;
    let token = read_token(&config_dir)?;
    let client = DaemonClient::new(config.daemon.host.clone(), config.daemon.port, token);

    let event = build_event(
        session_id,
        source,
        workspace,
        EventType::StatusHint,
        json!({
            "status": status,
            "origin": "hook",
        }),
    );
    client.post_event(&event).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_core_claude_hooks_to_six_states() {
        assert_eq!(status_for_hook_event("UserPromptSubmit"), Some("thinking"));
        assert_eq!(status_for_hook_event("PreToolUse"), Some("executing"));
        assert_eq!(status_for_hook_event("PostToolUse"), Some("thinking"));
        assert_eq!(status_for_hook_event("Notification"), Some("waiting"));
        assert_eq!(status_for_hook_event("PreCompact"), Some("compacting"));
        assert_eq!(status_for_hook_event("Stop"), Some("completed"));
        assert_eq!(status_for_hook_event("SessionStart"), Some("idle"));
    }

    #[test]
    fn unknown_hook_events_are_ignored() {
        assert_eq!(status_for_hook_event("SessionEnd"), None);
        assert_eq!(status_for_hook_event("FileChanged"), None);
        assert_eq!(status_for_hook_event(""), None);
    }

    #[test]
    fn tool_failure_and_batch_return_to_thinking() {
        assert_eq!(
            status_for_hook_event("PostToolUseFailure"),
            Some("thinking")
        );
        assert_eq!(status_for_hook_event("PostToolBatch"), Some("thinking"));
    }
}
