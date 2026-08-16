use promptchi_core::scoring::{pontuar, ContextoSessao, TipoPrompt, VERSAO_ENGINE};

/// (texto, prompts_anteriores, tipo esperado)
/// As notas não são fixadas caso a caso de propósito: o que este teste
/// protege são as INVARIANTES e a estabilidade da assinatura agregada.
const CASOS: [(&str, usize, TipoPrompt); 24] = [
    ("cria o endpoint de listagem de usuarios", 0, TipoPrompt::Inicial),
    ("implementa cache em src/api/users.ts sem alterar a assinatura", 0, TipoPrompt::Inicial),
    ("qual a melhor abordagem para isso?", 0, TipoPrompt::Inicial),
    ("", 0, TipoPrompt::Inicial),
    ("prossiga", 1, TipoPrompt::Continuacao),
    ("continue", 5, TipoPrompt::Continuacao),
    ("sim", 2, TipoPrompt::Continuacao),
    ("ok pode ir", 3, TipoPrompt::Continuacao),
    ("   ", 4, TipoPrompt::Continuacao),
    ("nao, era o outro arquivo", 2, TipoPrompt::Correcao),
    ("errado, reverta", 3, TipoPrompt::Correcao),
    ("volta o que voce fez em core/src/lib.rs", 4, TipoPrompt::Correcao),
    ("por que o teste quebrou?", 2, TipoPrompt::Pergunta),
    ("isso funciona em windows?", 6, TipoPrompt::Pergunta),
    ("qual o impacto de mudar parseLine?", 3, TipoPrompt::Pergunta),
    ("adiciona paginacao no endpoint", 1, TipoPrompt::Followup),
    ("corrige isso", 2, TipoPrompt::Followup),
    ("corrige o null check em auth/middleware.ts", 2, TipoPrompt::Followup),
    ("refatora o modulo mantendo a interface publica", 3, TipoPrompt::Followup),
    ("devolve o resultado em json", 2, TipoPrompt::Followup),
    ("roda src/main.rs", 1, TipoPrompt::Followup),
    ("por favor, se possivel, obrigado", 2, TipoPrompt::Followup),
    ("move UserService para services/", 4, TipoPrompt::Followup),
    ("apenas o codigo, sem explicacao", 3, TipoPrompt::Followup),
];

fn ctx(n: usize) -> ContextoSessao {
    ContextoSessao { prompts_anteriores: n }
}

#[test]
fn tipos_classificados_como_esperado() {
    for (texto, anteriores, esperado) in CASOS {
        let s = pontuar(texto, &ctx(anteriores));
        assert_eq!(s.tipo, esperado, "tipo errado para {texto:?} com {anteriores} anteriores");
    }
}

#[test]
fn todas_as_notas_no_intervalo() {
    for (texto, anteriores, _) in CASOS {
        let s = pontuar(texto, &ctx(anteriores));
        assert!(s.valor <= 100, "nota {} fora do intervalo para {texto:?}", s.valor);
    }
}

#[test]
fn todas_carregam_a_versao_corrente() {
    for (texto, anteriores, _) in CASOS {
        assert_eq!(pontuar(texto, &ctx(anteriores)).versao, VERSAO_ENGINE);
    }
}

/// Assinatura agregada do comportamento. Qualquer mudança de nota em
/// qualquer caso altera esta soma; se isso for intencional, bump
/// `VERSAO_ENGINE` e atualize o valor abaixo NA MESMA mudança.
#[test]
fn assinatura_agregada_congelada() {
    let soma: u32 = CASOS.iter().map(|(t, n, _)| pontuar(t, &ctx(*n)).valor as u32).sum();
    assert_eq!(
        VERSAO_ENGINE, 1,
        "engine mudou de versao: atualize a soma esperada abaixo junto"
    );
    // Rode uma vez, veja o valor real na falha, e fixe aqui.
    assert_eq!(soma, SOMA_ESPERADA_V1, "score mudou sem bump de VERSAO_ENGINE");
}

/// Preenchido na primeira execução (Step 2 do plano).
const SOMA_ESPERADA_V1: u32 = 1529;

/// Invariantes que precisam valer em qualquer versão da engine.
#[test]
fn invariantes_de_calibracao() {
    let com_ancora = pontuar("corrige o null check em auth/middleware.ts", &ctx(2)).valor;
    let so_deitico = pontuar("corrige isso", &ctx(2)).valor;
    assert!(com_ancora > so_deitico, "ancora concreta deve superar deitico puro");

    let continuacao = pontuar("prossiga", &ctx(2)).valor;
    assert!(continuacao >= 50, "continuacao legitima nao pode ser punida");

    let cortesia = pontuar("por favor, se possivel, obrigado", &ctx(2)).valor;
    let objetivo = pontuar("move UserService para services/", &ctx(2)).valor;
    assert!(objetivo > cortesia, "conteudo deve superar cortesia vazia");
}
