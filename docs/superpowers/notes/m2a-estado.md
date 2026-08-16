# M2a — Estado ao fim da implementação

**Data:** 2026-08-16
**Branch:** `feat/m2a-score-estrutural` (13 commits sobre `main`)
**Testes:** 135 verdes. Build release sem avisos.

---

## O que foi entregue

Score estrutural determinístico, visível ao vivo no watcher.

| Módulo | Entrega |
|---|---|
| `core/src/scoring/tipo.rs` | classifica em inicial, continuação, correção, pergunta, followup |
| `core/src/scoring/sinais.rs` | extrai âncoras, restrições, formato pedido, dêiticos, ruído |
| `core/src/scoring/rubrica.rs` | pondera por tipo; dimensão irrelevante recebe peso zero |
| `core/src/scoring/mod.rs` | `pontuar()`, `Score`, `VERSAO_ENGINE` |
| `core/tests/golden_scoring.rs` | snapshot nota a nota dos 24 casos |
| `cli/src/main.rs` | watcher exibe tipo e nota; média por provider no resumo |

---

## Pendente: dogfood humano

O Step 5 da Task 6 não foi executado — exige julgamento humano.

```bash
cargo run --release --bin promptchi-watch
```

Envie prompts de naturezas diferentes: um com caminho de arquivo, um `prossiga`,
uma pergunta. Confirme que o tipo bate com a natureza, que o prompt com âncora
recebe nota maior que a continuação, e que a média aparece no resumo.

Registre em `docs/superpowers/notes/m2a-dogfood.md`. **Onde discordar de uma
nota, anote qual e por quê** — é esse desacordo que vira o dataset rotulado que
o spec pede e que hoje não existe.

### O que a revisão final espera que você descubra

**Colapso numérico.** Prompts iniciais bem formados mas sem âncora convergem
todos para **38**. Followups sem âncora, para **57**. Perguntas sem âncora, para
**65**. Vai parecer que a engine não está julgando nada na maioria dos seus
prompts reais.

Causa: especificidade pesa 0.40 em prompt inicial — o maior peso de qualquer
dimensão — e vale zero sempre que não há âncora concreta. O corpus mede âncora
em apenas 35% dos prompts. Dois terços colapsam no mesmo número.

Isso é **calibração, não bug**. A válvula de escape está prescrita: se as notas
contrariarem seu julgamento de forma sistemática, o ajuste correto é mexer nos
pesos em `rubrica.rs` e **bump de `VERSAO_ENGINE`**, atualizando o array do
snapshot junto. Nunca relaxar os testes.

**Onde olhar se uma nota surpreender.** O campo `Score.dimensoes` decompõe a
nota, mas hoje não é impresso — só `valor` e `tipo`. Vale imprimi-lo durante o
dogfood. Especificidade zero domina os casos baixos; âncora inesperada explica
os altos demais.

**Teste de propósito** um prompt com data (`24/08`) e um com ponto colado sem
espaço (`bug.Depois`) — eram falsos positivos de âncora, corrigidos na onda
final, e vale confirmar em uso real.

---

## Débito conhecido, adjudicado

**Conjunções de 3 letras com barra ainda produzem âncora falsa.** `"ele/ela"`,
`"seu/sua"` contam como caminho de arquivo e inflam a nota em cerca de 20
pontos. A heurística exige um segmento com 3 ou mais caracteres não numéricos,
e esses pares passam.

O comentário em `sinais.rs` cita `ele/ela` como alvo da correção — a afirmação
é imprecisa e deve ser corrigida junto com o comportamento.

Ruling: real, estreito, nada a jusante depende disso, e o processo não previa
segunda onda de correção. Fica para o M2b, junto do resto do trabalho em
`sinais.rs`.

---

## Dívida triada para depois

**Corrigir cedo:**
- imprimir `Score.dimensoes` no watcher — sem isso, nota surpreendente é
  indepurável em uso real
- duplicação do conceito "parece caminho" entre `tipo.rs` e `sinais.rs`, com
  precisões diferentes; risco de divergirem

**Pode esperar:**
- `soma_notas` usa `u32` enquanto `Contadores` usa `u64`
- `prompts_por_sessao` sem teto, mesmo padrão já aceito em `cursores`/`mtimes`
- teste da média asserta presença da substring, não o valor
- média trunca em vez de arredondar
- `cargo fmt` diverge em `rubrica.rs` — divergência é do repositório inteiro,
  não deste branch

---

## Nota de processo

Cinco das seis tasks precisaram de rodada de correção, e **todos os defeitos
vieram do texto do plano**, não de transcrição errada pelos implementadores.
Três eram a mesma família: código escrito e testado como se o corpus fosse em
inglês sem pontuação.

| Task | Defeito |
|---|---|
| 1 | ponto final fazia `"sim."` deixar de ser continuação |
| 2 | ponto final fazia `"main.rs."` deixar de ser âncora; depois, offset em bytes usado como índice de caractere |
| 3 | multiplicação antes de limitar, com pânico por overflow |
| 5 | soma agregada mascarava troca compensatória de notas entre casos |
| 6 | o trecho só incrementava a soma, nunca a contagem — a média nunca imprimiria |

Nenhum foi encontrado por leitura. Cada um apareceu porque um revisor rodou o
código com entradas próprias: acentuação, valores extremos, remoção temporária
do bloco sob teste.

A revisão final foi despachada em modelo de tier inferior ao que o processo
prescreve, por limite de sessão — desvio consciente, registrado aqui.
