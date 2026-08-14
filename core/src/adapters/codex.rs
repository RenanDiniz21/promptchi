use serde_json::Value;

use crate::adapters::PromptSource;
use crate::types::{PromptEvent, Provider};

pub struct CodexAdapter;

impl PromptSource for CodexAdapter {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn parse_line(&self, line: &str, session_id: &str) -> Option<PromptEvent> {
        let v: Value = serde_json::from_str(line).ok()?;

        if v.get("type")?.as_str()? != "event_msg" {
            return None;
        }
        let payload = v.get("payload")?;
        if payload.get("type")?.as_str()? != "user_message" {
            return None;
        }

        let texto = payload.get("message")?.as_str()?.trim();
        if texto.is_empty() {
            return None;
        }

        Some(PromptEvent {
            provider: Provider::Codex,
            session_id: session_id.to_string(),
            text: texto.to_string(),
            timestamp: v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string(),
            cwd: None,
        })
    }
}

/// Extrai o UUID de sessão de `rollout-<timestamp>-<uuid>.jsonl`.
/// O UUID são os últimos 5 segmentos separados por hífen, no formato 8-4-4-4-12.
pub fn sessao_do_caminho(path: &str) -> Option<String> {
    let arquivo = path.rsplit(['/', '\\']).next()?;
    let base = arquivo.strip_suffix(".jsonl")?;
    let partes: Vec<&str> = base.split('-').collect();
    if partes.len() < 5 {
        return None;
    }
    let uuid: Vec<&str> = partes[partes.len() - 5..].to_vec();
    let tamanhos = [8usize, 4, 4, 4, 12];
    for (parte, esperado) in uuid.iter().zip(tamanhos.iter()) {
        if parte.len() != *esperado || !parte.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
    }
    Some(uuid.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::PromptSource;

    fn a() -> CodexAdapter { CodexAdapter }

    #[test]
    fn user_message_e_capturado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:40.258Z","type":"event_msg","payload":{"type":"user_message","client_id":"abc","message":"muda a sequencia de refugo\n","images":[]}}"#;
        let e = a().parse_line(l, "s9").expect("deveria capturar");
        assert_eq!(e.text, "muda a sequencia de refugo");
        assert_eq!(e.provider, Provider::Codex);
        assert_eq!(e.timestamp, "2026-06-24T15:52:40.258Z");
    }

    #[test]
    fn agent_message_e_ignorado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:41.000Z","type":"event_msg","payload":{"type":"agent_message","message":"resposta"}}"#;
        assert!(a().parse_line(l, "s9").is_none());
    }

    #[test]
    fn response_item_e_ignorado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:41.000Z","type":"response_item","payload":{"type":"function_call","name":"shell"}}"#;
        assert!(a().parse_line(l, "s9").is_none());
    }

    #[test]
    fn session_meta_e_ignorado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:40.182Z","type":"session_meta","payload":{"session_id":"abc","cwd":"C:\\proj"}}"#;
        assert!(a().parse_line(l, "s9").is_none());
    }

    #[test]
    fn json_invalido_nao_causa_panic() {
        assert!(a().parse_line("{quebrado", "s9").is_none());
    }

    #[test]
    fn extrai_sessao_do_nome_do_arquivo() {
        let p = "C:/Users/x/.codex/archived_sessions/rollout-2026-06-24T12-52-17-019efa54-f763-7691-91d3-c9a38153c864.jsonl";
        assert_eq!(
            sessao_do_caminho(p).as_deref(),
            Some("019efa54-f763-7691-91d3-c9a38153c864")
        );
    }

    #[test]
    fn nome_sem_uuid_retorna_none() {
        assert_eq!(sessao_do_caminho("C:/x/qualquer.jsonl"), None);
    }
}
