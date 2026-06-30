pub fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.chars().any(char::is_whitespace) {
                format!("\"{}\"", part.replace('"', "\\\""))
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_command_for_payload() {
        let command = vec![
            "codex".to_string(),
            "-m".to_string(),
            "gpt-5 codex".to_string(),
        ];

        assert_eq!(shell_join(&command), "codex -m \"gpt-5 codex\"");
    }
}
