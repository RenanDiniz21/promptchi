pub mod rubrica;
pub mod sinais;
pub mod tipo;

pub use rubrica::{avaliar, Dimensoes, TETO_CONTINUACAO};
pub use sinais::{extrair, Sinais};
pub use tipo::{classificar, ContextoSessao, TipoPrompt};

/// Versão do algoritmo de score. Qualquer mudança de comportamento exige
/// bump — o snapshot de regressão (Task 5) quebra o build sem isso.
pub const VERSAO_ENGINE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    pub valor: u8,
    pub tipo: TipoPrompt,
    pub dimensoes: Dimensoes,
    pub versao: u32,
}

/// Função pura de `(texto, contexto, versão)`. Sem relógio, sem aleatoriedade,
/// sem ambiente — determinismo é requisito de confiança, não de engenharia:
/// o usuário perdoa nota levemente injusta, não perdoa notas diferentes para
/// o mesmo prompt.
pub fn pontuar(texto: &str, ctx: &ContextoSessao) -> Score {
    let tipo = classificar(texto, ctx);
    let sinais = extrair(texto);
    let (valor, dimensoes) = avaliar(tipo, &sinais);
    Score { valor, tipo, dimensoes, versao: VERSAO_ENGINE }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(n: usize) -> ContextoSessao {
        ContextoSessao { prompts_anteriores: n }
    }

    #[test]
    fn pontuar_e_funcao_pura() {
        let t = "corrige o null check em auth/middleware.ts";
        let primeiro = pontuar(t, &ctx(3));
        for _ in 0..500 {
            assert_eq!(pontuar(t, &ctx(3)), primeiro, "score nao pode variar entre chamadas");
        }
    }

    #[test]
    fn score_carrega_versao_e_tipo() {
        let s = pontuar("prossiga", &ctx(2));
        assert_eq!(s.versao, VERSAO_ENGINE);
        assert_eq!(s.tipo, TipoPrompt::Continuacao);
    }

    #[test]
    fn contexto_muda_o_tipo_e_pode_mudar_a_nota() {
        let t = "prossiga";
        assert_eq!(pontuar(t, &ctx(0)).tipo, TipoPrompt::Inicial);
        assert_eq!(pontuar(t, &ctx(1)).tipo, TipoPrompt::Continuacao);
    }

    #[test]
    fn nota_sempre_valida_em_entrada_arbitraria() {
        for t in ["", "   ", "?", "\n\n", "a", &"x ".repeat(5000)] {
            for n in [0usize, 1, 50] {
                let s = pontuar(t, &ctx(n));
                assert!(s.valor <= 100);
            }
        }
    }
}
