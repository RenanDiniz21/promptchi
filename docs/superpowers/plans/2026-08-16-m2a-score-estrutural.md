# Promptchi M2a — Score Estrutural: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transformar cada prompt capturado numa nota 0–100 determinística, classificada por tipo, visível ao vivo no watcher.

**Architecture:** Módulo `scoring` dentro do crate puro `core`. Um prompt é primeiro **classificado** por tipo (inicial, continuação, correção, pergunta, followup), depois tem **sinais estruturais** extraídos do texto, e por fim uma **rubrica específica do tipo** combina os sinais em nota. Dimensão irrelevante para o tipo recebe peso zero — nunca nota baixa. Tudo é função pura de `(texto, contexto, versão)`.

**Tech Stack:** Rust 1.97.1, crate `promptchi-core` (só `serde`/`serde_json`), `tempfile` em dev.

## Global Constraints

- `core/` é crate **puro**: sem Tauri, sem I/O de rede, sem dependência de SO, sem acesso a disco. Só `serde` e `serde_json` em produção.
- Score é **função pura** de `(texto, contexto do turno, versão da engine)`. Mesma entrada, mesma saída, sempre. Nenhuma fonte de aleatoriedade, tempo ou ambiente.
- Nenhuma regra de negócio em JavaScript.
- Nunca `panic!`, nunca `unwrap()` sobre dado externo.
- Toda nota fica no intervalo fechado `0..=100`.
- `VERSAO_ENGINE` acompanha todo score produzido; mudança de comportamento sem bump de versão quebra o build.
- Mensagens de commit em português, prefixo convencional (`feat:`, `test:`, `chore:`).

---

## Calibração: o que a medição do corpus real determinou

Medido sobre **1.833 prompts humanos** em 497 arquivos de transcript, extraídos pelos adapters já validados. Estes números não são estimativa — são a base de projeto desta engine.

| | Claude Code (611) | Codex (1222) |
|---|---|---|
| Mediana de caracteres | 76 | 63 |
| Mediana de palavras | 13 | 10 |
| p90 de caracteres | 824 | 305 |
| Máximo | 27.430 | 5.610 |
| < 20 caracteres | 14% | 22% |
| ≤ 2 palavras | 11% | 17% |
| Com âncora concreta | 35% | 32% |
| Com bloco de código | 4% | 0% |
| Pergunta | 23% | 23% |
| Correção explícita | 2% | 3% |
| Primeira palavra imperativa | 5% | 3% |

**Cinco consequências que o implementador precisa respeitar:**

1. **Comprimento não pontua.** A mediana é de 10–13 palavras e esses prompts funcionam — construíram os milestones anteriores deste projeto. Nenhuma dimensão pode recompensar texto mais longo. A rubrica original do documento de produto reprovaria a mediana do usuário, e isso ensinaria o hábito errado.
2. **`Continuacao` não é caso de borda: é 11–17%.** Um em cada seis prompts tem duas palavras ou menos. Classificar errado aqui castiga um sexto da base.
3. **Âncora concreta é discriminador real:** só um terço dos prompts tem. Serve como sinal, mas com peso diferente por tipo — `prossiga` não precisa de âncora.
4. **Bloco de código é inútil como sinal:** 0–4%. Não entra na rubrica.
5. **Correção explícita é rara: 2–3%.** A detecção de correção existe para classificar o tipo, não para servir de sinal de qualidade.

---

## File Structure

```
core/src/scoring/
  mod.rs        API pública: Score, pontuar(), VERSAO_ENGINE
  tipo.rs       TipoPrompt + classificar()
  sinais.rs     Sinais + extrair() — análise pura de texto
  rubrica.rs    pesos por tipo, combinação → nota
core/tests/
  golden_scoring.rs    snapshot de determinismo
cli/src/main.rs        exibe a nota na saída do watcher
```

Separação por responsabilidade: `sinais` não sabe o que é nota; `rubrica` não sabe ler texto; `tipo` não sabe pontuar. Cada um é testável isolado.

---

### Task 1: Tipo de prompt e classificação

**Files:**
- Create: `core/src/scoring/mod.rs`, `core/src/scoring/tipo.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: nada
- Produces:
  - `TipoPrompt` — enum com variantes `Inicial`, `Continuacao`, `Correcao`, `Pergunta`, `Followup`, com `as_str()` retornando `"inicial"`, `"continuacao"`, `"correcao"`, `"pergunta"`, `"followup"`
  - `ContextoSessao { pub prompts_anteriores: usize }`
  - `classificar(texto: &str, ctx: &ContextoSessao) -> TipoPrompt`

- [ ] **Step 1: Escrever os testes que falham**

`core/src/scoring/tipo.rs`:

```rust
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
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core tipo
```

Esperado: erro de compilação, `cannot find type TipoPrompt`.

- [ ] **Step 3: Implementar**

No topo de `core/src/scoring/tipo.rs`:

```rust
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
    // Só conta como âncora o ponto que faz parte de extensão ou identificador,
    // ou seja, seguido de alfanumérico. Ponto final de frase não é âncora —
    // "sim." e "ok." são continuações legítimas, e são a forma mais comum
    // delas. O '.' é ASCII de 1 byte, então fatiar em `i + 1` é fronteira
    // válida em UTF-8.
    let ponto_de_extensao = baixo
        .char_indices()
        .any(|(i, c)| c == '.' && baixo[i + 1..].starts_with(|p: char| p.is_alphanumeric()));
    if baixo.contains('/') || baixo.contains('\\') || ponto_de_extensao {
        return false;
    }
    palavras.iter().all(|p| {
        let limpo = p.trim_matches(|c: char| !c.is_alphanumeric());
        PALAVRAS_CONTINUACAO.contains(&limpo)
            || ["a", "o", "e", "com", "isso", "agora", "por", "favor"].contains(&limpo)
    })
}
```

`core/src/scoring/mod.rs`:

```rust
pub mod tipo;

pub use tipo::{classificar, ContextoSessao, TipoPrompt};
```

Adicionar em `core/src/lib.rs`, sem remover nada:

```rust
pub mod scoring;
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core tipo
```

Esperado: `8 passed`.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "feat: classificacao de tipo de prompt"
```

---

### Task 2: Sinais estruturais

Extrai do texto só o que parsing enxerga de verdade. Nenhum julgamento semântico.

**Files:**
- Create: `core/src/scoring/sinais.rs`
- Modify: `core/src/scoring/mod.rs`

**Interfaces:**
- Consumes: nada
- Produces:
  - `Sinais { pub palavras: usize, pub ancoras: usize, pub restricoes: usize, pub formato_pedido: bool, pub deiticos: usize, pub ruido: usize }`
  - `extrair(texto: &str) -> Sinais`

- [ ] **Step 1: Escrever os testes que falham**

`core/src/scoring/sinais.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_palavras() {
        assert_eq!(extrair("uma duas tres").palavras, 3);
        assert_eq!(extrair("").palavras, 0);
    }

    #[test]
    fn caminho_de_arquivo_e_ancora() {
        assert_eq!(extrair("corrige src/main.rs").ancoras, 1);
        assert_eq!(extrair("veja core\\src\\lib.rs agora").ancoras, 1);
    }

    #[test]
    fn identificador_camelcase_e_ancora() {
        assert_eq!(extrair("o metodo parseLine falhou").ancoras, 1);
    }

    #[test]
    fn texto_comum_nao_tem_ancora() {
        assert_eq!(extrair("melhora isso um pouco por favor").ancoras, 0);
    }

    #[test]
    fn detecta_restricoes() {
        assert!(extrair("faz isso sem usar regex").restricoes >= 1);
        assert!(extrair("mantenha a assinatura atual").restricoes >= 1);
        assert_eq!(extrair("cria um endpoint").restricoes, 0);
    }

    #[test]
    fn detecta_formato_pedido() {
        assert!(extrair("me devolve em json").formato_pedido);
        assert!(extrair("responde em uma tabela").formato_pedido);
        assert!(extrair("apenas o codigo, sem explicacao").formato_pedido);
        assert!(!extrair("corrige o bug").formato_pedido);
    }

    #[test]
    fn conta_deiticos() {
        assert_eq!(extrair("corrige isso").deiticos, 1);
        assert_eq!(extrair("muda isso e aquilo").deiticos, 2);
        assert_eq!(extrair("corrige src/main.rs").deiticos, 0);
    }

    #[test]
    fn conta_ruido_de_cortesia() {
        assert!(extrair("por favor, se possivel, obrigado").ruido >= 2);
        assert_eq!(extrair("corrige o teste").ruido, 0);
    }

    #[test]
    fn texto_vazio_nao_causa_panic() {
        let s = extrair("");
        assert_eq!(s.palavras, 0);
        assert_eq!(s.ancoras, 0);
        assert!(!s.formato_pedido);
    }
}
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core sinais
```

Esperado: erro de compilação, `cannot find function extrair`.

- [ ] **Step 3: Implementar**

No topo de `core/src/scoring/sinais.rs`:

```rust
/// Extensões de arquivo que contam como âncora concreta.
const EXTENSOES: [&str; 14] = [
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".cs", ".java", ".go", ".json", ".md", ".yml",
    ".yaml", ".toml",
];

/// Marcadores de restrição explícita.
const RESTRICOES: [&str; 10] = [
    "sem ", "não use", "nao use", "evite", "mantenha", "apenas ", "somente ", "no máximo",
    "no maximo", "em vez de",
];

/// Marcadores de formato de saída pedido.
const FORMATOS: [&str; 9] = [
    "em json", "em yaml", "em markdown", "em tabela", "uma tabela", "em lista", "só o código",
    "so o codigo", "apenas o codigo",
];

/// Pronomes sem antecedente explícito.
const DEITICOS: [&str; 8] = ["isso", "isto", "aquilo", "esse", "essa", "aquele", "aquela", "ele"];

/// Cortesia que não carrega informação.
const RUIDO: [&str; 6] = ["por favor", "se possivel", "se possível", "obrigado", "valeu", "please"];

/// Só o que parsing de fato enxerga. Nenhum julgamento semântico aqui.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sinais {
    pub palavras: usize,
    pub ancoras: usize,
    pub restricoes: usize,
    pub formato_pedido: bool,
    pub deiticos: usize,
    pub ruido: usize,
}

pub fn extrair(texto: &str) -> Sinais {
    let baixo = texto.to_lowercase();
    let palavras: Vec<&str> = texto.split_whitespace().collect();

    let ancoras = palavras
        .iter()
        .filter(|p| e_ancora(p))
        .count();

    let restricoes = RESTRICOES.iter().filter(|m| baixo.contains(**m)).count();
    let formato_pedido = FORMATOS.iter().any(|m| baixo.contains(*m));
    let ruido = RUIDO.iter().filter(|m| baixo.contains(**m)).count();

    let deiticos = palavras
        .iter()
        .filter(|p| {
            let limpo = p.to_lowercase();
            let limpo = limpo.trim_matches(|c: char| !c.is_alphanumeric());
            DEITICOS.contains(&limpo)
        })
        .count();

    Sinais { palavras: palavras.len(), ancoras, restricoes, formato_pedido, deiticos, ruido }
}

/// Âncora é referência concreta: caminho, arquivo com extensão conhecida, ou
/// identificador em camelCase/PascalCase. Prosa comum não produz âncora — no
/// corpus real só um terço dos prompts tem alguma.
fn e_ancora(palavra: &str) -> bool {
    let baixo = palavra.to_lowercase();
    if EXTENSOES.iter().any(|e| baixo.ends_with(e) || baixo.contains(&format!("{e}:"))) {
        return true;
    }
    if palavra.contains('/') || palavra.contains('\\') {
        return true;
    }
    // camelCase / PascalCase: tem minúscula e uma maiúscula depois da primeira posição.
    let tem_minuscula = palavra.chars().any(|c| c.is_lowercase());
    let maiuscula_interna = palavra.chars().skip(1).any(|c| c.is_uppercase());
    palavra.len() > 3 && tem_minuscula && maiuscula_interna
}
```

Adicionar em `core/src/scoring/mod.rs`:

```rust
pub mod sinais;

pub use sinais::{extrair, Sinais};
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core sinais
```

Esperado: `9 passed`.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "feat: extracao de sinais estruturais do prompt"
```

---

### Task 3: Rubrica por tipo

Converte sinais em nota. Aqui mora a decisão de projeto mais importante: **dimensão irrelevante para o tipo recebe peso zero, nunca nota baixa.**

**Files:**
- Create: `core/src/scoring/rubrica.rs`
- Modify: `core/src/scoring/mod.rs`

**Interfaces:**
- Consumes: `TipoPrompt` (Task 1), `Sinais` (Task 2)
- Produces:
  - `Dimensoes { pub especificidade: u8, pub restricoes: u8, pub formato: u8, pub clareza: u8, pub concisao: u8 }`
  - `avaliar(tipo: TipoPrompt, s: &Sinais) -> (u8, Dimensoes)` — nota final e as dimensões que a compuseram

- [ ] **Step 1: Escrever os testes que falham**

`core/src/scoring/rubrica.rs`:

```rust
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
}
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core rubrica
```

Esperado: erro de compilação, `cannot find function avaliar`.

- [ ] **Step 3: Implementar**

No topo de `core/src/scoring/rubrica.rs`:

```rust
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
    100u8.saturating_sub((orfaos * 30).min(70) as u8)
}

/// Penaliza cortesia vazia e verbosidade sem conteúdo. Não recompensa texto
/// curto por ser curto: um prompt de 5 palavras sem ruído fica em 100.
fn concisao(s: &Sinais) -> u8 {
    let mut n = 100u8.saturating_sub((s.ruido * 20).min(60) as u8);
    let conteudo = s.ancoras + s.restricoes;
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
```

Adicionar em `core/src/scoring/mod.rs`:

```rust
pub mod rubrica;

pub use rubrica::{avaliar, Dimensoes, TETO_CONTINUACAO};
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core rubrica
```

Esperado: `7 passed`.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "feat: rubrica de score por tipo de prompt"
```

---

### Task 4: API pública e versionamento

**Files:**
- Modify: `core/src/scoring/mod.rs`

**Interfaces:**
- Consumes: `classificar`, `extrair`, `avaliar`
- Produces:
  - `VERSAO_ENGINE: u32` — começa em `1`
  - `Score { pub valor: u8, pub tipo: TipoPrompt, pub dimensoes: Dimensoes, pub versao: u32 }`
  - `pontuar(texto: &str, ctx: &ContextoSessao) -> Score`

- [ ] **Step 1: Escrever os testes que falham**

Ao final de `core/src/scoring/mod.rs`:

```rust
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
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core scoring
```

Esperado: erro de compilação, `cannot find function pontuar`.

- [ ] **Step 3: Implementar**

Substituir o conteúdo de `core/src/scoring/mod.rs` acima do bloco `mod tests` por:

```rust
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
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core scoring
```

Esperado: `4 passed`, e a suíte inteira do crate verde.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "feat: api publica de scoring com versionamento"
```

---

### Task 5: Snapshot de regressão

Congela o comportamento. Qualquer mudança de nota sem bump de versão quebra o build.

**Files:**
- Create: `core/tests/golden_scoring.rs`

**Interfaces:**
- Consumes: `pontuar`, `ContextoSessao`, `VERSAO_ENGINE`
- Produces: nada — é o gate de determinismo

> **Desvio do spec, deliberado.** O spec pede ~100 prompts **rotulados à mão**.
> Não existe rotulagem humana ainda, e inventar 100 rótulos seria fabricar
> autoridade que o dado não tem. Este snapshot usa 24 casos sintéticos
> cobrindo o espaço de tipos e as fronteiras, e congela a nota **atual** —
> serve para detectar mudança não intencional, não para provar que a nota é
> justa. A rotulagem humana entra quando existir o botão "discordo" previsto
> no spec, e aí o dataset cresce com rótulo de verdade.

- [ ] **Step 1: Escrever o teste**

`core/tests/golden_scoring.rs`:

```rust
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
const SOMA_ESPERADA_V1: u32 = 0;

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
```

- [ ] **Step 2: Rodar, ler a soma real e fixá-la**

```bash
cargo test -p promptchi-core --test golden_scoring
```

O teste `assinatura_agregada_congelada` vai falhar mostrando a soma real
(`left: <valor>, right: 0`). Substitua `SOMA_ESPERADA_V1 = 0` por esse valor.

- [ ] **Step 3: Rodar de novo e confirmar tudo verde**

```bash
cargo test -p promptchi-core --test golden_scoring
```

Esperado: `5 passed`.

- [ ] **Step 4: Rodar a suíte inteira**

```bash
cargo test --workspace
```

Esperado: todos os testes anteriores continuam verdes.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "test: snapshot de regressao do scoring"
```

---

### Task 6: Score visível no watcher

Torna o M2a dogfoodável: cada prompt capturado passa a sair com tipo e nota.

**Files:**
- Modify: `cli/src/main.rs`

**Interfaces:**
- Consumes: `pontuar`, `ContextoSessao`, `Score` de `promptchi_core::scoring`
- Produces: saída do watcher com tipo e nota; contador de nota média por provider

- [ ] **Step 1: Contar prompts por sessão**

Em `cli/src/main.rs`, no struct `Estado`, adicione um campo para saber quantos prompts humanos já foram vistos em cada sessão — é o que `ContextoSessao` precisa:

```rust
prompts_por_sessao: HashMap<(Provider, String), usize>,
```

Inicialize junto dos demais campos do `Estado`.

- [ ] **Step 2: Pontuar no ponto de emissão**

No laço de emissão, logo após obter `evento` e passar pela deduplicação, e **antes** do `println!`, substitua o bloco de impressão por:

```rust
let chave = (provider, sessao.clone());
let anteriores = *self.prompts_por_sessao.get(&chave).unwrap_or(&0);
let score = pontuar(&evento.text, &ContextoSessao { prompts_anteriores: anteriores });
self.prompts_por_sessao.insert(chave, anteriores + 1);

self.contadores.entry(provider).or_default().eventos += 1;
self.soma_notas.entry(provider).or_default().0 += score.valor as u32;
self.total += 1;
let preview: String = evento.text.chars().take(70).collect();
println!(
    "[{:>4}] {:<12} {:>3}  {:<12} {}",
    self.total,
    evento.provider.as_str(),
    score.valor,
    score.tipo.as_str(),
    preview
);
```

Adicione `soma_notas: HashMap<Provider, (u32, usize)>` ao `Estado` — soma das notas e quantidade, para calcular a média sem guardar todas.

Ajuste o import no topo do arquivo:

```rust
use promptchi_core::scoring::{pontuar, ContextoSessao};
```

- [ ] **Step 3: Incluir a média no resumo periódico**

Em `imprimir_resumo`, para cada provider que tiver pelo menos um prompt, acrescente a nota média à linha já existente:

```rust
if let Some((soma, n)) = self.soma_notas.get(&provider) {
    if *n > 0 {
        print!(" | nota media: {}", soma / *n as u32);
    }
}
```

Mantenha o resto da linha como está.

- [ ] **Step 4: Compilar e verificar**

```bash
cargo build --workspace
cargo test --workspace
```

Esperado: build sem erros, todos os testes verdes.

- [ ] **Step 5: Verificação manual**

```bash
cargo run --release --bin promptchi-watch
```

Envie três prompts de naturezas diferentes num agente suportado — um com caminho de arquivo, um `prossiga`, e uma pergunta — e confirme na saída que:

1. o tipo classificado bate com a natureza do prompt
2. o prompt com caminho de arquivo recebe nota maior que o `prossiga`
3. a linha de resumo mostra a nota média

Registre o resultado em `docs/superpowers/notes/m2a-dogfood.md`, incluindo os três prompts, os tipos e as notas — e se alguma nota contrariar seu julgamento, anote qual e por quê. É esse desacordo que vira o dataset rotulado de verdade.

- [ ] **Step 6: Commit**

```bash
git add cli docs/superpowers/notes/m2a-dogfood.md
git commit -m "feat: watcher exibe tipo e nota de cada prompt"
```

---

## Fora deste plano

**M2b — sinais de resultado.** Montagem de turno, detecção de pedido de esclarecimento, retrabalho e aceitação, e a combinação em banda de ±20 sobre a nota estrutural. Precisa de estado de turno, que este plano não introduz.

**M2c — baseline e percentil.** Backfill do histórico, distribuição congelada do "você inicial", e normalização da nota bruta em percentil pessoal. Depende de persistência, que ainda não existe.

**Feedback textual por LLM (BYOK).** Não altera nota; só gera a frase explicativa. Depende de configuração e de filtro de secrets.

Cada um vira seu próprio plano, e cada um produz software funcionando sozinho.

## Correções aplicadas durante a execução

O código-fonte em `core/src/scoring/` é a **fonte de verdade**. Os blocos deste
plano ficam como registro do ponto de partida, e divergem nos pontos abaixo —
todos defeitos deste texto, encontrados na revisão e corrigidos.

| Onde | Defeito | Correção |
|---|---|---|
| Task 1, `e_continuacao` | `contains('.')` tratava ponto final de frase como âncora de extensão; `"sim."` deixava de ser continuação | só conta ponto seguido de alfanumérico |
| Task 2, `e_ancora` | `ends_with(extensao)` falhava com pontuação colada; `"main.rs."` não era âncora | apara pontuação do fim antes de testar extensão |
| Task 2, marcadores | `RESTRICOES`/`RUIDO` casavam substring sem fronteira; `"prevaleu"` contava como `"valeu"` | helper de fronteira de palavra nas duas pontas |
| Task 3, `clareza`/`concisao` | multiplicavam antes de limitar, com pânico por overflow | `saturating_mul` / `saturating_add` |
| Task 5, snapshot | soma agregada mascarava troca compensatória de notas entre casos | snapshot nota a nota, mensagem nomeia o caso |
| Task 6, resumo | o trecho só incrementava a soma, nunca a contagem — média nunca imprimiria | incrementa os dois; `print!` trocado por `push_str`, coerente com a arquitetura real |
| Revisão final, `e_ancora` | `"bug.Depois"` passava na heurística camelCase; `"24/08"` e `"e/ou"` passavam como caminho | camelCase por segmento; barra exige extensão ou segmento com 3+ caracteres não numérico |
| Revisão final, `tem_fronteira_palavra` | `pos = start + 1` assumia caractere de 1 byte; constante futura com acento causaria pânico | avança por `len_utf8()` do caractere casado |

Estado e débito conhecido em
[`m2a-estado.md`](../notes/m2a-estado.md).

## Nota de calibração para quem executar

Os pesos da rubrica na Task 3 são **hipótese calibrada**, não verdade. Foram
escolhidos para que a mediana medida do corpus real — 10–13 palavras, um
terço com âncora — não seja punida. Se durante o dogfood da Task 6 as notas
contrariarem seu julgamento de forma sistemática, o ajuste correto é mexer
nos pesos e **bump da versão**, não relaxar os testes.
