pub mod claude_code;
// `pub mod codex;` é adicionado na Task 6 — declarar aqui antes do arquivo
// existir quebra a compilação desta task.

use crate::types::{PromptEvent, Provider};

/// Converte uma linha de transcript no evento canônico.
/// Retorna `None` para qualquer linha que não seja prompt autorado por humano,
/// inclusive JSON inválido ou formato desconhecido.
pub trait PromptSource {
    fn provider(&self) -> Provider;
    fn parse_line(&self, line: &str, session_id: &str) -> Option<PromptEvent>;
}
