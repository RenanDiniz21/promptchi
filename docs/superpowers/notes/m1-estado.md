# M1 — Estado ao fim da implementação

**Data:** 2026-08-15
**Branch:** `feat/m0-m1-fundacao` (20 commits sobre `main`)
**Testes:** 73 verdes (22 cli + 45 core + 6 integração). Build release sem avisos.

---

## O que foi entregue

Núcleo de captura completo, em Rust puro:

| Módulo | Entrega |
|---|---|
| `core/src/types.rs` | `Provider`, `PromptEvent` |
| `core/src/jsonl.rs` | `LineBuffer` — só emite linhas completas |
| `core/src/cursor.rs` | `FileCursor` — leitura incremental por offset |
| `core/src/adapters/claude_code.rs` | discriminação de prompt humano |
| `core/src/adapters/codex.rs` | idem, mais estado de sessão |
| `cli/src/main.rs` | `promptchi-watch`, watcher + revarredura |
| `cli/src/dedup.rs` | deduplicação restrita a forks do Codex |

---

## Pendências que exigem você

### 1. M0 — spike de overlay (Tasks 1 e 2 do plano)

Não executado. Exige julgamento visual que subagente não pode fazer: transparência
de WebView2 sobre desktop vivo, grade do spritesheet, click-through.

**É gate eliminatório.** Se a transparência falhar, o design do overlay muda antes
de qualquer investimento em UI.

### 2. Gate de M1 — um dia de uso real (Task 9, Steps 4 e 5)

```bash
cargo run --release --bin promptchi-watch
```

Critério: **zero prompt perdido, zero duplicado, zero crash** ao longo de um dia
de trabalho.

Método que a revisão recomendou, e que hoje é possível: comparar os **contadores
de diagnóstico** (linhas lidas, eventos produzidos, rejeitadas — por provider)
com a percepção real de uso. Sem esses contadores, "capturei 200 prompts" seria
indistinguível de "li 40 mil linhas e rejeitei todas".

Registrar o resultado em `docs/superpowers/notes/m1-gate.md`.

**Onde olhar se falhar:**

| Sintoma | Primeiro lugar |
|---|---|
| Duplicata | `cli/src/main.rs` (chave do cursor) e `session_meta` do arquivo culpado |
| Prompt fantasma | filtros em `adapters/codex.rs` e `adapters/claude_code.rs` |
| Prompt perdido | `core/src/cursor.rs` (offset/cauda) e log de erro de `read_dir` |
| Máquina lenta | intervalo de revarredura em `cli/src/main.rs` |

---

## Limitações aceitas conscientemente

**Impressão digital de 32 bytes.** Detecta substituição de arquivo sem depender de
API de sistema operacional. Medição real derrubou a premissa original: os 237
arquivos do Claude Code têm apenas **3 prefixos distintos** — poder discriminante
quase nulo desse lado. O risco efetivo continua baixo porque os nomes são UUID e
não há colisão de `file_stem` entre diretórios de projeto. Endurecimento barato
para o futuro: `set_path` só é necessário para o Codex, que é quem move arquivos;
chavear o Claude Code pelo caminho completo eliminaria a classe inteira.

**Dedup em sessão forkada.** Dois prompts idênticos digitados dentro da mesma
sessão forkada do Codex contam como duplicado. Frequência medida: 1 sessão em 71
dias de corpus.

**Sessão do Codex sem `session_meta` observado não emite.** Escolha conservadora,
porque 105 de 206 arquivos são de subagente e presumir "humano" contaminaria a
base. A perda entra no contador e dispara aviso nomeando sessão e arquivo. Nos
207 arquivos medidos, `session_meta` é a linha 1 em 207.

**Contrato de `registrar_sessao`.** Quem usa `CodexAdapter` precisa chamá-lo em
toda linha, não só nas que vai emitir. Esquecer zera a captura do Codex inteira.
Hoje há ponto de chamada único e um teste que falha se ele sumir. Tornar o
contrato inescapável por tipo fica para M2.

---

## Dívida triada para M2

**Corrigir cedo:**
- `PromptEvent` sem campo `seq` — M2 classifica `initial` como "primeiro turno
  humano da sessão" e precisa de ordem estável
- `cwd` sempre `None` no Codex, embora `session_meta.payload.cwd` exista
- contador `rejeitadas` mistura "linha que nunca foi prompt" com "rejeitada por
  filtro" — é justamente o segundo número que revelaria degradação de formato

**Pode esperar:**
- `PromptSource::provider()` não é usado em produção
- `timestamp` vazio degrada mudo
- corrida de arranque: watcher registrado antes do posicionamento dos cursores
- `Estado.mtimes` mantém entrada de arquivo apagado
- doc comments ausentes nos tipos públicos
- sem `.gitattributes` (CRLF)

---

## Nota de processo

A última correção — restringir a dedup a forks do Codex, mais duas quebras
menores — foi verificada por suíte de testes e por medição do implementador
(pico de RSS de volta a 610 MB, arranque em 1,4 s, fumaça ponta a ponta dos
quatro cenários), **mas não passou por uma rodada independente de revisão**. O
processo prevê uma única onda de correção após a revisão final, e ela já havia
sido gasta. Se quiser cobertura completa, uma revisão dos commits
`fa9addd..58b39c8` fecharia essa lacuna.
