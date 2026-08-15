use std::collections::HashMap;

use serde_json::Value;

use crate::adapters::PromptSource;
use crate::types::{PromptEvent, Provider};

/// Substring barata que identifica candidatas a `session_meta` sem pagar o
/// custo de desserializar a linha. Serve para varrer arquivos inteiros
/// (centenas de MB no corpus real) procurando só o cabeçalho.
const MARCA_SESSION_META: &str = "\"session_meta\"";

/// O que o `session_meta` conta sobre a sessão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoSessao {
    pub origem: OrigemSessao,
    /// `payload.forked_from_id` presente: a sessão foi criada por fork de
    /// outra e abre reemitindo o histórico da sessão-pai. É a única
    /// situação em que a captura vê o mesmo prompt duas vezes, e por isso a
    /// única em que a deduplicação por conteúdo se justifica.
    pub forkada: bool,
}

/// De onde vem uma sessão do Codex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrigemSessao {
    /// `thread_source == "user"`: conversa digitada por gente.
    Humana,
    /// `thread_source == "subagent"` (ou os sinais equivalentes em
    /// `payload.source.subagent` / `payload.parent_thread_id`): a thread foi
    /// aberta pelo próprio agente. Os `user_message` dela são prompts que o
    /// Codex escreveu para si mesmo, não do usuário.
    Subagente,
}

/// Adaptador do Codex.
///
/// Diferente do Claude Code, que marca cada linha com `isSidechain`, o Codex
/// só diz que a thread é de subagente UMA vez, no `session_meta` que abre o
/// arquivo. `parse_line` vê uma linha por vez, então o adaptador guarda a
/// origem de cada sessão já observada.
///
/// IMPORTANTE para quem consome: chame [`CodexAdapter::registrar_sessao`] em
/// toda linha lida, inclusive nas que forem descartadas (posicionamento de
/// cursor no arranque, por exemplo). Sem isso a origem da sessão nunca é
/// registrada e os prompts dela são descartados por precaução.
#[derive(Debug, Default)]
pub struct CodexAdapter {
    sessoes: HashMap<String, InfoSessao>,
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self { sessoes: HashMap::new() }
    }

    /// Se a linha for o `session_meta` da sessão, registra a origem dela.
    /// Idempotente e barata para linhas que não são `session_meta`.
    pub fn registrar_sessao(&mut self, line: &str, session_id: &str) {
        if !line.contains(MARCA_SESSION_META) {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        if v.get("type").and_then(Value::as_str) != Some("session_meta") {
            return;
        }
        let Some(payload) = v.get("payload") else { return };
        let info = InfoSessao {
            origem: if e_subagente(payload) { OrigemSessao::Subagente } else { OrigemSessao::Humana },
            forkada: payload.get("forked_from_id").is_some_and(|v| !v.is_null()),
        };
        self.sessoes.insert(session_id.to_string(), info);
    }

    /// O que se sabe da sessão, ou `None` se o `session_meta` dela ainda não
    /// passou por [`CodexAdapter::registrar_sessao`].
    pub fn info(&self, session_id: &str) -> Option<InfoSessao> {
        self.sessoes.get(session_id).copied()
    }

    /// Origem já conhecida da sessão, ou `None` se o `session_meta` dela
    /// ainda não passou por [`CodexAdapter::registrar_sessao`].
    pub fn origem(&self, session_id: &str) -> Option<OrigemSessao> {
        self.info(session_id).map(|i| i.origem)
    }

    /// `true` só para sessão criada por fork de outra — a única que reemite
    /// histórico e, portanto, a única que precisa de deduplicação.
    pub fn e_fork(&self, session_id: &str) -> bool {
        self.info(session_id).is_some_and(|i| i.forkada)
    }
}

/// Sinais de thread de subagente no `payload` do `session_meta`. Os três
/// coincidem nos 105 de 206 arquivos de subagente do corpus medido; qualquer
/// um basta, para o filtro sobreviver a renomeação de campo.
fn e_subagente(payload: &Value) -> bool {
    if payload.get("thread_source").and_then(Value::as_str) == Some("subagent") {
        return true;
    }
    if payload.get("source").and_then(Value::as_object).is_some_and(|o| o.contains_key("subagent")) {
        return true;
    }
    payload.get("parent_thread_id").is_some_and(|v| !v.is_null())
}

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

        // Sessão de subagente não emite NENHUM prompt. Sessão cuja origem
        // ainda não foi observada também não: o `session_meta` é sempre a
        // primeira linha do arquivo, então ler um `user_message` sem
        // conhecer a origem significa que o arquivo foi aberto no meio — e
        // 105 dos 206 arquivos do corpus são de subagente, ou seja, chutar
        // "é humano" contaminaria a captura na maioria dos casos. O
        // consumidor detecta a situação com `origem() == None` e registra.
        if self.origem(session_id) != Some(OrigemSessao::Humana) {
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

    const META_HUMANA: &str = r#"{"timestamp":"2026-06-24T15:52:40.182Z","type":"session_meta","payload":{"id":"abc","cwd":"C:\\proj","source":"vscode","thread_source":"user"}}"#;
    const META_SUBAGENTE: &str = r#"{"timestamp":"2026-06-09T12:15:35.683Z","type":"session_meta","payload":{"id":"abc","parent_thread_id":"019e9328-a975-74a2-a87f-bc56a31198a2","cwd":"C:\\proj","source":{"subagent":{"other":"guardian"}},"thread_source":"subagent"}}"#;
    const USER_MESSAGE: &str = r#"{"timestamp":"2026-06-24T15:52:40.258Z","type":"event_msg","payload":{"type":"user_message","client_id":"abc","message":"muda a sequencia de refugo\n","images":[]}}"#;

    /// Adaptador com a sessão `s9` já reconhecida como humana.
    fn a() -> CodexAdapter {
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(META_HUMANA, "s9");
        ad
    }

    #[test]
    fn user_message_e_capturado() {
        let e = a().parse_line(USER_MESSAGE, "s9").expect("deveria capturar");
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
        assert!(a().parse_line(META_HUMANA, "s9").is_none());
        assert!(a().parse_line(META_SUBAGENTE, "s9").is_none());
    }

    #[test]
    fn json_invalido_nao_causa_panic() {
        assert!(a().parse_line("{quebrado", "s9").is_none());
        let mut ad = a();
        ad.registrar_sessao("{quebrado \"session_meta\"", "s9");
        assert_eq!(ad.origem("s9"), Some(OrigemSessao::Humana));
    }

    #[test]
    fn sessao_de_subagente_nao_emite_nenhum_prompt() {
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(META_SUBAGENTE, "sub");
        assert_eq!(ad.origem("sub"), Some(OrigemSessao::Subagente));
        assert!(ad.parse_line(USER_MESSAGE, "sub").is_none(), "prompt de subagente vazou");
    }

    #[test]
    fn thread_spawn_tambem_e_subagente() {
        let l = r#"{"type":"session_meta","payload":{"id":"x","parent_thread_id":"p","thread_source":"subagent","source":{"subagent":{"thread_spawn":{"parent_thread_id":"p","depth":1,"agent_path":null}}}}}"#;
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(l, "sub");
        assert_eq!(ad.origem("sub"), Some(OrigemSessao::Subagente));
    }

    #[test]
    fn parent_thread_id_sozinho_marca_subagente() {
        // Rede de segurança caso `thread_source` mude de nome.
        let l = r#"{"type":"session_meta","payload":{"id":"x","parent_thread_id":"p"}}"#;
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(l, "sub");
        assert_eq!(ad.origem("sub"), Some(OrigemSessao::Subagente));
    }

    #[test]
    fn sessao_sem_meta_observado_nao_emite() {
        // Escolha conservadora documentada: sem `session_meta`, não dá para
        // saber se a thread é de subagente, e a maioria dos arquivos é.
        let ad = CodexAdapter::new();
        assert_eq!(ad.origem("desconhecida"), None);
        assert!(ad.parse_line(USER_MESSAGE, "desconhecida").is_none());
    }

    #[test]
    fn origem_e_por_sessao_e_nao_global() {
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(META_SUBAGENTE, "sub");
        ad.registrar_sessao(META_HUMANA, "hum");
        assert!(ad.parse_line(USER_MESSAGE, "sub").is_none());
        assert!(ad.parse_line(USER_MESSAGE, "hum").is_some());
    }

    #[test]
    fn fork_de_sessao_humana_continua_humana() {
        let l = r#"{"type":"session_meta","payload":{"id":"x","forked_from_id":"y","source":"vscode","thread_source":"user"}}"#;
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(l, "fork");
        assert_eq!(ad.origem("fork"), Some(OrigemSessao::Humana));
        assert!(ad.parse_line(USER_MESSAGE, "fork").is_some());
    }

    #[test]
    fn forked_from_id_marca_a_sessao_como_fork() {
        let l = r#"{"type":"session_meta","payload":{"id":"x","forked_from_id":"y","source":"vscode","thread_source":"user"}}"#;
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(l, "fork");
        assert!(ad.e_fork("fork"));
        assert_eq!(
            ad.info("fork"),
            Some(InfoSessao { origem: OrigemSessao::Humana, forkada: true })
        );
    }

    #[test]
    fn sessao_sem_forked_from_id_nao_e_fork() {
        let mut ad = CodexAdapter::new();
        ad.registrar_sessao(META_HUMANA, "s9");
        assert!(!ad.e_fork("s9"));
        assert!(!ad.e_fork("sessao_que_nunca_vi"));

        // `forked_from_id: null` tambem nao conta.
        let l = r#"{"type":"session_meta","payload":{"id":"x","forked_from_id":null,"thread_source":"user"}}"#;
        ad.registrar_sessao(l, "nula");
        assert!(!ad.e_fork("nula"));
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
