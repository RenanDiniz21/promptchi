use crate::scoring::sinais::Sinais;
use crate::scoring::tipo::TipoPrompt;

/// Continuação legítima não é punida, mas também não chega ao topo enquanto
/// não existir o sinal de resultado (M2b). Sem esse teto, "sim" valeria 100.
pub const TETO_CONTINUACAO: u8 = 70;

/// Notas por dimensão, na escala 0..=100. Dimensão com peso zero no tipo
/// aparece aqui como 0 — é ausência de julgamento, não julgamento negativo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dimensoes {
    pub especificidade: u8,
    pub restricoes: u8,
    pub formato: u8,
    pub clareza: u8,
    pub concisao: u8,
}

/// Pesos por tipo. Somam 1.0 dentro de cada tipo; peso zero exclui a
/// dimensão do cálculo em vez de arrastar a nota para baixo.
struct Pesos {
    especificidade: f32,
    restricoes: f32,
    formato: f32,
    clareza: f32,
    concisao: f32,
}

fn pesos(tipo: TipoPrompt) -> Pesos {
    match tipo {
        TipoPrompt::Inicial => Pesos {
            especificidade: 0.40,
            restricoes: 0.15,
            formato: 0.10,
            clareza: 0.20,
            concisao: 0.15,
        },
        TipoPrompt::Followup => Pesos {
            especificidade: 0.30,
            restricoes: 0.10,
            formato: 0.05,
            clareza: 0.35,
            concisao: 0.20,
        },
        TipoPrompt::Pergunta => Pesos {
            especificidade: 0.35,
            restricoes: 0.0,
            formato: 0.0,
            clareza: 0.40,
            concisao: 0.25,
        },
        // Continuação e correção não pedem contexto novo nem formato: o
        // turno anterior já estabeleceu os dois.
        TipoPrompt::Continuacao => {
            Pesos { especificidade: 0.0, restricoes: 0.0, formato: 0.0, clareza: 0.5, concisao: 0.5 }
        }
        TipoPrompt::Correcao => Pesos {
            especificidade: 0.25,
            restricoes: 0.0,
            formato: 0.0,
            clareza: 0.60,
            concisao: 0.15,
        },
    }
}

pub fn avaliar(tipo: TipoPrompt, s: &Sinais) -> (u8, Dimensoes) {
    let p = pesos(tipo);

    let esp = if p.especificidade > 0.0 { especificidade(s) } else { 0 };
    let res = if p.restricoes > 0.0 { escala(s.restricoes, 2) } else { 0 };
    let fmt = if p.formato > 0.0 {
        if s.formato_pedido { 100 } else { 30 }
    } else {
        0
    };
    let cla = if p.clareza > 0.0 { clareza(s) } else { 0 };
    let con = if p.concisao > 0.0 { concisao(s) } else { 0 };

    let bruto = esp as f32 * p.especificidade
        + res as f32 * p.restricoes
        + fmt as f32 * p.formato
        + cla as f32 * p.clareza
        + con as f32 * p.concisao;

    let mut nota = bruto.round().clamp(0.0, 100.0) as u8;
    if tipo == TipoPrompt::Continuacao && nota > TETO_CONTINUACAO {
        nota = TETO_CONTINUACAO;
    }

    (nota, Dimensoes { especificidade: esp, restricoes: res, formato: fmt, clareza: cla, concisao: con })
}

/// Âncoras concretas, saturando rápido: duas já indicam alvo bem definido.
/// Deliberadamente independente do comprimento — no corpus real a mediana
/// tem 10–13 palavras e funciona.
fn especificidade(s: &Sinais) -> u8 {
    escala(s.ancoras, 2)
}

/// Penaliza pronome sem âncora que o sustente. "corrige isso" com uma âncora
/// ao lado é claro; sozinho, não é.
fn clareza(s: &Sinais) -> u8 {
    let orfaos = s.deiticos.saturating_sub(s.ancoras);
    100u8.saturating_sub(orfaos.saturating_mul(30).min(70) as u8)
}

/// Penaliza cortesia vazia e verbosidade sem conteúdo. Não recompensa texto
/// curto por ser curto: um prompt de 5 palavras sem ruído fica em 100.
fn concisao(s: &Sinais) -> u8 {
    let mut n = 100u8.saturating_sub(s.ruido.saturating_mul(20).min(60) as u8);
    let conteudo = s.ancoras.saturating_add(s.restricoes);
    if s.palavras > 40 && conteudo == 0 {
        n = n.saturating_sub(30);
    }
    n
}

/// Mapeia contagem para 0..=100 saturando em `alvo`.
fn escala(valor: usize, alvo: usize) -> u8 {
    if alvo == 0 {
        return 0;
    }
    ((valor.min(alvo) * 100) / alvo) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::sinais::extrair;
    use crate::scoring::tipo::TipoPrompt;

    fn nota(tipo: TipoPrompt, texto: &str) -> u8 {
        avaliar(tipo, &extrair(texto)).0
    }

    #[test]
    fn nota_sempre_no_intervalo() {
        for t in [
            TipoPrompt::Inicial,
            TipoPrompt::Followup,
            TipoPrompt::Pergunta,
            TipoPrompt::Continuacao,
            TipoPrompt::Correcao,
        ] {
            for texto in ["", "isso", "corrige src/main.rs sem usar regex, devolve em json"] {
                let n = nota(t, texto);
                assert!(n <= 100, "nota {n} fora do intervalo para {t:?} / {texto:?}");
            }
        }
    }

    #[test]
    fn ancora_concreta_supera_deitico_puro() {
        let com = nota(TipoPrompt::Followup, "corrige o null check em auth/middleware.ts");
        let sem = nota(TipoPrompt::Followup, "corrige isso");
        assert!(com > sem, "com ancora {com} deveria superar deitico {sem}");
    }

    #[test]
    fn comprimento_sozinho_nao_aumenta_nota() {
        let curto = nota(TipoPrompt::Inicial, "cria o endpoint em api/users.ts sem autenticacao");
        let inchado = nota(
            TipoPrompt::Inicial,
            "por favor, se possivel, eu gostaria muito que voce criasse para mim, com todo \
             cuidado e atencao, algo que sirva de alguma forma, obrigado desde ja",
        );
        assert!(curto > inchado, "curto com conteudo {curto} deveria superar inchado {inchado}");
    }

    #[test]
    fn continuacao_nao_e_punida_e_nao_estoura() {
        let n = nota(TipoPrompt::Continuacao, "prossiga");
        assert!(n >= 50, "continuacao legitima nao pode ser punida, veio {n}");
        assert!(n <= TETO_CONTINUACAO, "continuacao nao pode chegar ao topo sem resultado, veio {n}");
    }

    #[test]
    fn formato_e_restricao_nao_pesam_em_continuacao() {
        let d = avaliar(TipoPrompt::Continuacao, &extrair("prossiga")).1;
        assert_eq!(d.formato, 0, "formato deveria ter peso zero, nao nota baixa");
        assert_eq!(d.restricoes, 0, "restricoes deveria ter peso zero, nao nota baixa");
    }

    #[test]
    fn dimensoes_relevantes_aparecem_no_detalhe() {
        let d = avaliar(
            TipoPrompt::Inicial,
            &extrair("cria o endpoint em api/users.ts sem autenticacao, devolve em json"),
        )
        .1;
        assert!(d.especificidade > 0);
        assert!(d.restricoes > 0);
        assert!(d.formato > 0);
    }

    #[test]
    fn texto_vazio_nao_causa_panic() {
        let _ = avaliar(TipoPrompt::Inicial, &extrair(""));
        let _ = avaliar(TipoPrompt::Continuacao, &extrair("   "));
    }

    #[test]
    fn formato_e_restricao_nao_pesam_em_pergunta_e_correcao() {
        let s = extrair("sem usar regex, devolve em json");
        for t in [TipoPrompt::Pergunta, TipoPrompt::Correcao] {
            let d = avaliar(t, &s).1;
            assert_eq!(d.formato, 0, "formato deveria ter peso zero em {t:?}, nao nota baixa");
            assert_eq!(d.restricoes, 0, "restricoes deveria ter peso zero em {t:?}, nao nota baixa");
        }
    }

    #[test]
    fn sinais_com_valores_extremos_nao_causa_panic() {
        let s = Sinais {
            palavras: usize::MAX,
            ancoras: usize::MAX,
            restricoes: usize::MAX,
            formato_pedido: true,
            deiticos: usize::MAX,
            ruido: usize::MAX,
        };
        for t in [
            TipoPrompt::Inicial,
            TipoPrompt::Followup,
            TipoPrompt::Pergunta,
            TipoPrompt::Continuacao,
            TipoPrompt::Correcao,
        ] {
            let (n, d) = avaliar(t, &s);
            assert!(n <= 100, "nota {n} fora do intervalo para {t:?} com sinais extremos");
            assert!(d.especificidade <= 100);
            assert!(d.restricoes <= 100);
            assert!(d.formato <= 100);
            assert!(d.clareza <= 100);
            assert!(d.concisao <= 100);
        }
    }
}
