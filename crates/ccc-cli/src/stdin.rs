use std::io::{self, IsTerminal, Read};

pub fn read_stdin_if_piped() -> io::Result<Option<String>> {
    if io::stdin().is_terminal() {
        return Ok(None);
    }

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(if buffer.trim().is_empty() {
        None
    } else {
        Some(buffer)
    })
}

pub fn merge_prompt_and_stdin(prompt: Option<&str>, stdin: Option<&str>) -> Option<String> {
    let prompt = normalize_source(prompt);
    let stdin = normalize_source(stdin);

    match (prompt, stdin) {
        (Some(prompt), Some(stdin)) => Some(format!("{prompt}\n\n{stdin}")),
        (Some(prompt), None) => Some(prompt),
        (None, Some(stdin)) => Some(stdin),
        (None, None) => None,
    }
}

fn normalize_source(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.trim().is_empty() {
        return None;
    }

    Some(value.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_prompt_and_stdin_with_separator() {
        let merged = merge_prompt_and_stdin(Some("summarize this"), Some("hello\nworld"));
        assert_eq!(merged.as_deref(), Some("summarize this\n\nhello\nworld"));
    }

    #[test]
    fn keeps_prompt_when_stdin_missing() {
        let merged = merge_prompt_and_stdin(Some("summarize this"), None);
        assert_eq!(merged.as_deref(), Some("summarize this"));
    }

    #[test]
    fn keeps_stdin_when_prompt_missing() {
        let merged = merge_prompt_and_stdin(None, Some("hello"));
        assert_eq!(merged.as_deref(), Some("hello"));
    }

    #[test]
    fn returns_none_when_both_sources_empty() {
        let merged = merge_prompt_and_stdin(Some("   "), Some(" \n"));
        assert_eq!(merged, None);
    }
}
