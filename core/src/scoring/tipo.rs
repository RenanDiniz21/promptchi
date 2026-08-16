/// Marcadores que abrem uma correção do que o agente acabou de fazer.
/// Casados só no início do texto, após aparar espaços, em minúsculas.
const MARCADORES_CORRECAO: [&str; 8] =
    ["não", "nao", "errado", "erro:", "reverta", "volta", "desfaz", "no,"];

/// Palavras que, sozinhas ou quase, apenas mandam seguir.
const PALAVRAS_CONTINUACAO: [&str; 13] = [
    "prossiga", "continue", "continua", "continuar", "segue", "siga", "sim", "ok", "vai",
    "manda", "pode", "ir", "beleza",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoPrompt {
    Inicial,
    Continuacao,
    Correcao,
    Pergunta,
    Followup,
}

impl TipoPrompt {
    pub fn as_str(&self) -> &'static str {
        match self {
            TipoPrompt::Inicial => "inicial",
            TipoPrompt::Continuacao => "continuacao",
            TipoPrompt::Correcao => "correcao",
            TipoPrompt::Pergunta => "pergunta",
            TipoPrompt::Followup => "followup",
        }
    }
}

/// O mínimo que a classificação precisa saber sobre a sessão.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextoSessao {
    pub prompts_anteriores: usize,
}

/// Ordem de precedência deliberada:
/// 1. Sem turno anterior, é `Inicial` — mesmo sendo pergunta. O primeiro
///    prompt da sessão carrega o ônus de estabelecer contexto.
/// 2. Correção, porque revoga o turno anterior.
/// 3. Continuação: curto e sem âncora. São 11–17% do corpus real, então
///    errar aqui castiga um sexto da base.
/// 4. Pergunta.
/// 5. Followup para o resto.
pub fn classificar(texto: &str, ctx: &ContextoSessao) -> TipoPrompt {
    let t = texto.trim();
    if ctx.prompts_anteriores == 0 {
        return TipoPrompt::Inicial;
    }
    let baixo = t.to_lowercase();
    if MARCADORES_CORRECAO.iter().any(|m| comeca_com_palavra(&baixo, m)) {
        return TipoPrompt::Correcao;
    }
    if e_continuacao(&baixo) {
        return TipoPrompt::Continuacao;
    }
    if t.ends_with('?') {
        return TipoPrompt::Pergunta;
    }
    TipoPrompt::Followup
}

/// Casa `marcador` só como primeira palavra, para "não" não casar dentro de
/// "nãoentendi" nem "no," dentro de "nodejs".
fn comeca_com_palavra(baixo: &str, marcador: &str) -> bool {
    match baixo.strip_prefix(marcador) {
        Some(resto) => resto.is_empty() || resto.starts_with(|c: char| !c.is_alphanumeric()),
        None => false,
    }
}

/// Curto, sem âncora concreta, e feito só de palavras que mandam seguir.
/// Texto vazio conta como continuação: é o caso de um envio em branco.
fn e_continuacao(baixo: &str) -> bool {
    let palavras: Vec<&str> = baixo.split_whitespace().collect();
    if palavras.is_empty() {
        return true;
    }
    if palavras.len() > 3 {
        return false;
    }
    if baixo.contains('/') || baixo.contains('\\') || baixo.contains('.') {
        return false;
    }
    palavras.iter().all(|p| {
        let limpo = p.trim_matches(|c: char| !c.is_alphanumeric());
        PALAVRAS_CONTINUACAO.contains(&limpo)
            || ["a", "o", "e", "com", "isso", "agora", "por", "favor"].contains(&limpo)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(n: usize) -> ContextoSessao {
        ContextoSessao { prompts_anteriores: n }
    }

    #[test]
    fn primeiro_prompt_da_sessao_e_inicial() {
        assert_eq!(classificar("cria o endpoint de listagem", &ctx(0)), TipoPrompt::Inicial);
        assert_eq!(classificar("qual a melhor abordagem?", &ctx(0)), TipoPrompt::Inicial);
    }

    #[test]
    fn marcador_de_correcao_vence_depois_do_primeiro() {
        assert_eq!(classificar("não, era o outro arquivo", &ctx(3)), TipoPrompt::Correcao);
        assert_eq!(classificar("errado, reverta isso", &ctx(3)), TipoPrompt::Correcao);
    }

    #[test]
    fn prompt_curto_sem_ancora_e_continuacao() {
        assert_eq!(classificar("prossiga", &ctx(2)), TipoPrompt::Continuacao);
        assert_eq!(classificar("pode continuar", &ctx(2)), TipoPrompt::Continuacao);
        assert_eq!(classificar("sim", &ctx(2)), TipoPrompt::Continuacao);
    }

    #[test]
    fn prompt_curto_com_ancora_nao_e_continuacao() {
        assert_eq!(classificar("roda src/main.rs", &ctx(2)), TipoPrompt::Followup);
    }

    #[test]
    fn interrogacao_depois_do_primeiro_e_pergunta() {
        assert_eq!(classificar("por que isso quebrou?", &ctx(4)), TipoPrompt::Pergunta);
    }

    #[test]
    fn resto_e_followup() {
        assert_eq!(
            classificar("adiciona paginação no endpoint de listagem", &ctx(1)),
            TipoPrompt::Followup
        );
    }

    #[test]
    fn texto_vazio_nao_causa_panic() {
        assert_eq!(classificar("", &ctx(0)), TipoPrompt::Inicial);
        assert_eq!(classificar("   ", &ctx(5)), TipoPrompt::Continuacao);
    }

    #[test]
    fn nomes_estaveis() {
        assert_eq!(TipoPrompt::Inicial.as_str(), "inicial");
        assert_eq!(TipoPrompt::Continuacao.as_str(), "continuacao");
        assert_eq!(TipoPrompt::Correcao.as_str(), "correcao");
        assert_eq!(TipoPrompt::Pergunta.as_str(), "pergunta");
        assert_eq!(TipoPrompt::Followup.as_str(), "followup");
    }
}
