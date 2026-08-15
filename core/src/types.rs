#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    ClaudeCode,
    Codex,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::ClaudeCode => "claude_code",
            Provider::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptEvent {
    pub provider: Provider,
    pub session_id: String,
    pub text: String,
    pub timestamp: String,
    pub cwd: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_tem_nome_estavel() {
        assert_eq!(Provider::ClaudeCode.as_str(), "claude_code");
        assert_eq!(Provider::Codex.as_str(), "codex");
    }
}
