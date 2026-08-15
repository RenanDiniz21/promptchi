use serde_json::Value;

use crate::adapters::PromptSource;
use crate::types::{PromptEvent, Provider};

/// Textos que o Claude Code grava como se fossem mensagem do usuário mas que
/// ninguém digitou. Duas famílias, tratadas por `e_injecao_de_sistema`:
///
/// 1. `MARCADORES_LITERAIS` — o texto inteiro é o marcador.
/// 2. `TAGS_DE_SISTEMA` — o texto começa com uma tag de marcação que o
///    próprio Claude Code injeta. Medição sobre o corpus real (694 linhas
///    aceitas pelo filtro anterior): 101 injeções, ou 14,6%, distribuídas em
///    task-notification 54, command-name 15, ide_opened_file 14,
///    local-command-stdout 10, system-reminder 4, command-message 4.
///    `local-command-stdout` é a SAÍDA de um slash command, não entrada
///    digitada.
///
/// O casamento é ancorado no início do texto já aparado e restrito a estes
/// nomes de tag: um humano pode legitimamente colar um trecho começando com
/// `<`, e rejeitar qualquer marcação angular perderia prompt real.
const MARCADORES_LITERAIS: [&str; 2] = [
    "[Request interrupted by user]",
    "[Request interrupted by user for tool use]",
];

const TAGS_DE_SISTEMA: [&str; 6] = [
    "task-notification",
    "command-name",
    "command-message",
    "ide_opened_file",
    "local-command-stdout",
    "system-reminder",
];

fn e_injecao_de_sistema(texto: &str) -> bool {
    let t = texto.trim();
    if MARCADORES_LITERAIS.contains(&t) {
        return true;
    }
    let Some(resto) = t.strip_prefix('<') else { return false };
    TAGS_DE_SISTEMA.iter().any(|tag| {
        resto.strip_prefix(tag).is_some_and(|apos| {
            // A tag só casa se terminar aqui: `<tag>`, `<tag ...>` ou
            // `<tag/>`. Sem isso, `<command-namespace>` de um humano cairia
            // no filtro de `command-name`.
            apos.starts_with('>') || apos.starts_with('/') || apos.starts_with(char::is_whitespace)
        })
    })
}

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
        if e_injecao_de_sistema(&text) {
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

    fn linha_com_texto(texto: &str) -> String {
        let v = serde_json::json!({
            "type": "user",
            "isSidechain": false,
            "message": { "role": "user", "content": texto },
            "timestamp": "2026-07-15T13:00:00.000Z"
        });
        v.to_string()
    }

    #[test]
    fn tags_de_sistema_sao_rejeitadas() {
        let casos = [
            "<task-notification>\n<task-id>bapj26cky</task-id>\n</task-notification>",
            "<command-name>/model</command-name>\n<command-message>model</command-message>",
            "<command-message>ui-ux-pro-max</command-message>\n<command-name>/ui</command-name>",
            "<ide_opened_file>The user opened the file c:\\x.json in the IDE</ide_opened_file>",
            "<local-command-stdout>Set model to claude-sonnet-4-6</local-command-stdout>",
            "<system-reminder>\nThe user started your suggested background task\n</system-reminder>",
            // Espaço em branco antes da tag não escapa do filtro.
            "  \n<task-notification>x</task-notification>",
            // Tag com atributo continua sendo a mesma tag.
            "<system-reminder priority=\"high\">x</system-reminder>",
        ];
        for c in casos {
            assert!(
                a().parse_line(&linha_com_texto(c), "s1").is_none(),
                "deveria rejeitar injeção: {c}"
            );
        }
    }

    #[test]
    fn texto_humano_com_marcacao_angular_e_aceito() {
        let casos = [
            "<div class=\"card\"><span>Olá</span></div>",
            "<html>\n<body>teste</body>\n</html>",
            "<Button onClick={x}>salvar</Button>",
            // Prefixo parecido, mas não é a tag conhecida.
            "<command-namespace>o que é isso?</command-namespace>",
            "<task-notifications-config> revisa isso",
        ];
        for c in casos {
            let e = a()
                .parse_line(&linha_com_texto(c), "s1")
                .unwrap_or_else(|| panic!("deveria aceitar texto humano: {c}"));
            assert_eq!(e.text, c);
        }
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
