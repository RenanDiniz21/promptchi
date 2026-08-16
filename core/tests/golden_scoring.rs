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

/// Snapshot nota a nota do comportamento. Qualquer mudança de nota em
/// qualquer caso vai aparecer aqui; se isso for intencional, bump
/// `VERSAO_ENGINE` e atualize o array abaixo NA MESMA mudança.
/// A ordem segue exatamente CASOS[0..23].
#[test]
fn valores_congelados() {
    assert_eq!(
        VERSAO_ENGINE, 1,
        "engine mudou de versao: atualize o array de valores esperados abaixo junto"
    );
    for (i, (texto, anteriores, _)) in CASOS.iter().enumerate() {
        let obtido = pontuar(texto, &ctx(*anteriores)).valor;
        assert_eq!(
            obtido, VALORES_ESPERADOS_V1[i],
            "caso {} ({:?}): valor divergiu (esperado {}, obtido {})",
            i,
            texto,
            VALORES_ESPERADOS_V1[i],
            obtido
        );
    }
}

/// Preenchido na primeira execução. Array com os 24 valores, na ordem de CASOS.
/// Deve somar exatamente 1529 como verificação de integridade.
const VALORES_ESPERADOS_V1: [u8; 24] = [
    38, 66, 32, 38, 70, 70, 70, 70, 70, 75, 75, 88, 65, 53, 83, 57, 46, 72, 57, 60, 72, 45, 87, 70,
];

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
