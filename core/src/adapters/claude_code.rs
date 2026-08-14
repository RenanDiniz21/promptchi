use serde_json::Value;

use crate::adapters::PromptSource;
use crate::types::{PromptEvent, Provider};

const MARCADORES_DE_SISTEMA: [&str; 2] = [
    "[Request interrupted by user]",
    "[Request interrupted by user for tool use]",
];

pub struct ClaudeCodeAdapter;

impl PromptSource for ClaudeCodeAdapter {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn parse_line(&self, line: &str, session_id: &str) -> Option<PromptEvent> {
        let v: Value = serde_json::from_str(line).ok()?;

        if v.get("type")?.as_str()? != "user" {
            return None;
        }
        if v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }
        if v.get("isMeta").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }

        let message = v.get("message")?;
        if message.get("role")?.as_str()? != "user" {
            return None;
        }

        let text = extrair_texto_humano(message.get("content")?)?;
        if MARCADORES_DE_SISTEMA.contains(&text.as_str()) {
            return None;
        }

        Some(PromptEvent {
            provider: Provider::ClaudeCode,
            session_id: session_id.to_string(),
            text,
            timestamp: v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string(),
            cwd: v.get("cwd").and_then(Value::as_str).map(str::to_string),
        })
    }
}

/// `content` string = prompt digitado.
/// `content` array = aceito somente se não contiver nenhum bloco `tool_result`.
fn extrair_texto_humano(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        }
        Value::Array(blocos) => {
            let tem_tool_result = blocos
                .iter()
                .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"));
            if tem_tool_result {
                return None;
            }
            let partes: Vec<&str> = blocos
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if partes.is_empty() { None } else { Some(partes.join("\n")) }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::PromptSource;

    fn a() -> ClaudeCodeAdapter { ClaudeCodeAdapter }

    #[test]
    fn content_string_e_prompt_humano() {
        let l = r#"{"type":"user","message":{"role":"user","content":"corrige o null check"},"timestamp":"2026-07-15T13:00:00.000Z","cwd":"C:\\proj"}"#;
        let e = a().parse_line(l, "s1").expect("deveria capturar");
        assert_eq!(e.text, "corrige o null check");
        assert_eq!(e.session_id, "s1");
        assert_eq!(e.provider, Provider::ClaudeCode);
        assert_eq!(e.cwd.as_deref(), Some("C:\\proj"));
    }

    #[test]
    fn tool_result_e_ignorado() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn content_array_misto_com_tool_result_e_ignorado() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"corrige isso"},{"type":"tool_result","content":"ok"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn sidechain_e_ignorado() {
        let l = r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"do subagente"},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn is_meta_e_ignorado() {
        let l = r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn marcador_de_interrupcao_e_ignorado() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn array_apenas_com_text_e_aceito() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"primeira"},{"type":"text","text":"segunda"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        let e = a().parse_line(l, "s1").expect("deveria capturar");
        assert_eq!(e.text, "primeira\nsegunda");
    }

    #[test]
    fn assistant_e_ignorado() {
        let l = r#"{"type":"assistant","message":{"role":"assistant","content":"resposta"},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn json_invalido_nao_causa_panic() {
        assert!(a().parse_line("{quebrado", "s1").is_none());
        assert!(a().parse_line("", "s1").is_none());
    }
}
