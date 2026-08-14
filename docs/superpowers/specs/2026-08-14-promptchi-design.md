# Promptchi — Design Spec

**Data:** 2026-08-14
**Status:** aprovado para planejamento de implementação
**Origem:** revisão crítica de `promptchi-product-plan.md`

---

## 1. Resumo

Companion de desktop que observa prompts enviados a coding agents (Claude Code, Codex CLI), avalia a qualidade de cada um, converte a nota em comida para um mascote e faz o mascote refletir visualmente a qualidade recente da escrita do usuário.

Objetivo do produto: **ensinar a escrever prompts melhores**, tornando a melhora visível.

Objetivo do MVP: responder se o loop é divertido o suficiente para o usuário manter o app aberto.

---

## 2. Decisões fundamentais

| Decisão | Escolha | Motivo |
|---|---|---|
| Superfície de captura | Claude Code + Codex CLI | transcripts JSONL já em disco; captura robusta, offline, sem ToS |
| Motor de score | híbrido local + sinais de resultado | determinístico, gratuito, offline, honesto |
| LLM | opcional, BYOK, **não pontua** | só gera frase de feedback |
| Stack | Tauri v2 (core Rust + frontend web) | animação de sprite trivial, dashboard nativo, exe único |
| SO inicial | Windows | dogfooding diário do autor |
| Agência | ritual diário curto | vínculo sem obrigação |
| Mascote MVP | Kuriboh Alado (placeholder) | remove arte do caminho crítico |
| Contexto | produto próprio para lançar | otimizar por velocidade até primeiro usuário |

---

## 3. Correções ao plano original

### 3.1 Captura não é homogênea
O plano tratava Claude web e coding agents como integrações equivalentes. Não são. CLI agents escrevem transcripts em disco; web exige extensão e DOM frágil. **Captura via web/DOM foi descartada da arquitetura, não adiada.**

### 3.2 Score de prompt isolado é conceitualmente errado
O plano admitia (princípio 4.3) que `continue` pode ser excelente, mas listava dimensões que punem exatamente prompts curtos e eficientes. Risco: o produto ensina o hábito errado — prompts longos e cerimoniais para agradar a nota.

Correção: classificar o tipo do prompt antes de julgar, zerar peso de dimensões irrelevantes, e buscar verdade de campo no comportamento subsequente registrado no transcript.

### 3.3 Contradição privacidade × scoring semântico
Local-first e avaliação por LLM externo são incompatíveis. Resolvido tirando o LLM do caminho do score.

### 3.4 Atributos redundantes
Seis stats derivados de um único número. Colapsados em dois com semânticas ortogonais.

### 3.5 Escopo de arte irreal
~400 frames implícitos no MVP. Reduzido a ~90, e depois a **zero** com o asset existente.

### 3.6 Latência não tratada
Loop desenhado como síncrono. Resolvido com reação em duas fases.

---

## 4. Arquitetura

### 4.1 Fronteira principal

`promptchi-core` é crate Rust puro: sem Tauri, sem UI, sem rede. Entrada: bytes de JSONL. Saída: eventos de domínio. Testável headless.

`src-tauri` é casca fina: watcher, comandos IPC, janelas.

Frontend é apresentação. **Nenhuma regra de score em JavaScript** — seria não-determinístico, adulterável via devtools, e quebraria o requisito de ponto único de verdade versionado.

### 4.2 Módulos

| Módulo | Camada | Implementação |
|---|---|---|
| `watcher` | Rust | crate `notify` (usa `ReadDirectoryChangesW`) |
| `ingest` | core | cursor por byte, buffer de linha parcial |
| `adapters` | core | `serde_json`, trait `PromptSource` |
| `turns` | core | montagem e fechamento de turno |
| `privacy` | core | detecção e redação de secrets |
| `scoring` | core | heurística estrutural + ajuste por resultado |
| `progression` | core | comida, XP, fitness, nível |
| `store` | Rust | `rusqlite` feature `bundled` |
| `overlay` | frontend | sprite sheet via CSS `steps()` |
| `dashboard` | frontend | segunda janela decorada |

Fluxo: `watcher → ingest → adapters → turns → scoring → progression → store → render`

### 4.3 Threading

Thread do `notify` → canal → task do core (escritora única do SQLite) → `app.emit()` → frontend. Frontend nunca toca disco.

Volume real: dezenas de eventos por hora. Simplicidade vence throughput.

### 4.4 Portabilidade

Código específico de SO fica atrás de duas interfaces (`IFileWatcher`, janela). Port para macOS é troca de implementações, não reescrita.

---

## 5. Captura

### 5.1 Fontes

| Provider | Caminho | Registro relevante |
|---|---|---|
| Claude Code | `~/.claude/projects/<slug>/<sessionId>.jsonl` | `type:"queue-operation"`, `operation:"enqueue"` — texto literal digitado pelo humano |
| Codex | `~/.codex/sessions/<ano>/…`, `~/.codex/archived_sessions/rollout-*.jsonl` | regra equivalente a definir com amostra real em M1 |

O registro `queue-operation` do Claude Code resolve na origem a separação entre mensagem humana e `tool_result` injetado pelo sistema.

### 5.2 Mecânica

`ReadDirectoryChangesW` recursivo em duas raízes, thread dedicada, sem polling.

Por arquivo: cursor de byte persistido. Em cada notificação, ler do cursor até EOF, quebrar em linhas, **manter em buffer a última linha incompleta**. Escrita de JSONL não é atômica; avançar cursor apenas após linha terminada em `\n`.

### 5.3 Evento canônico

```
PromptEvent { session_id, provider, seq, text, timestamp, cwd }
```

Apenas turnos autorados por humano.

### 5.4 Degradação

Formato desconhecido deve degradar em silêncio e registrar diagnóstico. **Nunca crashar** — o app roda o dia inteiro em segundo plano. Adapters são versionados.

O app **apenas lê** os transcripts. Nunca escreve, move ou modifica.

---

## 6. Scoring Engine

### 6.1 Fase 0 — Classificação

| Tipo | Detecção | Rubrica |
|---|---|---|
| `initial` | primeiro turno humano da sessão | completa |
| `followup` | há turnos anteriores | contexto e restrições com peso zero |
| `continuation` | curto, sem conteúdo novo | apenas resultado |
| `correction` | sinaliza falha do agente | clareza da correção |
| `question` | interrogativo, sem pedido de ação | especificidade da pergunta |

Dimensão irrelevante recebe **peso zero, nunca nota baixa**.

### 6.2 Fase 1 — Estrutural (instantânea, <5ms, offline)

- **Especificidade referencial** — caminhos, símbolos, identificadores, números, nomes técnicos
- **Estrutura de tarefa** — verbo imperativo, escopo delimitado, enumeração, bloco de código
- **Restrições explícitas** — negações, limites, substituições
- **Formato pedido** — JSON, tabela, lista, "só o código"
- **Densidade informativa** — conteúdo útil por comprimento; **penaliza prompt longo e cerimonial**
- **Ambiguidade dêitica** — pronomes sem antecedente versus âncoras concretas
- **Ruído** — cortesia vazia, repetição, colagem sem propósito

Produz score provisório. Pet reage imediatamente.

### 6.3 Fase 2 — Resultado (diferida)

Turno fecha ao chegar o próximo prompt humano da mesma sessão, ou por timeout.

| Sinal | Detecção | Leitura |
|---|---|---|
| Correção seguinte | próximo prompt é `correction` | prompt anterior falhou |
| Pedido de esclarecimento | agente respondeu perguntando, sem tool use | faltou especificidade |
| Retrabalho | mesmos arquivos editados em turnos consecutivos | alvo mal definido |
| Aceitação | próximo prompt muda de assunto, ou sessão encerra limpa | funcionou |
| Custo do turno | nº de tool calls, duração | eficiência de direcionamento |
| Abandono | turno interrompido | rumo errado |

### 6.4 Combinação

```
final = clamp(estrutural + ajuste, 0, 100)
ajuste = confiança × Σ(peso_sinal × valor_sinal),  limitado a ±20
```

O estrutural define a nota base; o resultado a corrige dentro de uma banda de **±20 pontos**, escalada pela confiança.

Modelo de banda em vez de média ponderada porque o sinal de resultado é ruidoso — uma correção pode ser o usuário mudando de ideia, não prompt ruim. Peso majoritário amplificaria ruído; banda limitada corrige sem dominar.

Regras:

- turno sem resultado disponível (último da sessão) fica com o estrutural, marcado `partial`
- confiança gravada por evento; ajuste não é exibido ao usuário quando a confiança é baixa
- ajuste nunca é aplicado isoladamente sem score estrutural

### 6.5 Calibração por baseline congelada

O backfill do primeiro run constrói uma distribuição de referência — o "você inicial". Todo score subsequente é percentil contra essa baseline **fixa**.

Ganhos:
1. O número passa a significar algo verificável
2. A régua não se move, então melhora aparece como progressão real
3. Incorpora a métrica de valor do produto (`primeiros 20` vs `últimos 20`) na própria mecânica

### 6.6 Determinismo

- Score é **função pura** de `(texto, contexto do turno, versão da engine)`
- `scoring_engine_version` gravado em cada evento
- Dataset de regressão de ~100 prompts rotulados à mão; mudança de score sem bump de versão quebra o build
- Rescore em massa só é possível no modo com texto persistido; sem texto, histórico congela na versão original e é sinalizado na UI

### 6.7 LLM opcional

Escopo estrito: **gera apenas a frase de feedback**. Nunca altera o score.

Desligado por padrão. Exige chave do usuário. Passa pelo filtro de secrets. Rate-limited. Falha ou desligado → templates locais.

Consequência: o app funciona 100% offline e a nota nunca muda por causa da rede.

### 6.8 Exemplo

```
Prompt: "fix this"
  tipo: followup
  fase 1 → 34   (zero âncoras, dêitico puro)
  turno fecha: agente editou o arquivo certo, sem perguntar;
               próximo prompt mudou de assunto
  fase 2 → +18  (aceitação, sem esclarecimento, 1 tool call)
  final: 62 — "curto, mas o contexto carregava. Funcionou."
```

---

## 7. Pet e progressão

### 7.1 Dois atributos ortogonais

**Fitness (0–100)** — média móvel exponencial dos scores recentes. Reversível. Responde "como você está escrevendo agora".

**XP** — cumulativo, nunca decai. Controla nível e desbloqueios cosméticos. Responde "quanto você já praticou".

### 7.2 Decaimento por prompt, não por tempo

Meia-vida da EMA: **~30 prompts**, não dias.

Consequência: fim de semana, férias ou uma semana de reuniões não decaem nada. Só a qualidade da escrita move o ponteiro.

Motivo: punir ausência gera culpa, culpa gera desinstalação. Num app de trabalho isso é fatal.

### 7.3 Expressão visual do fitness

O asset do MVP tem um único estado físico, e "musculoso" não funciona para uma bola de pelo com asas. A metáfora muda de **massa muscular** para **vitalidade**.

| Fitness | Escala | Saturação | Ritmo do idle | Aura |
|---|---|---|---|---|
| 0–40 | 0.90× | 0.75 | lento, arrastado | nenhuma |
| 40–75 | 1.00× | 1.0 | normal | nenhuma |
| 75–100 | 1.12× | 1.1 + brilho | enérgico | `drop-shadow` dourado suave |

Adicionalmente: fitness baixo usa mais frames de `sleeping`/`sad` como idle variante; fitness alto usa mais frames de voo e agitação.

Tudo via CSS transform/filter. Zero asset novo. Estágios corporais reais ficam para V1.1.

### 7.4 Fórmulas

**Fitness (EMA):**

```
fitness ← fitness + α × (score_final − fitness)
α = 1 − 0.5^(1/30) ≈ 0.0228     -- meia-vida de 30 prompts
```

Aplicada **por prompt avaliado**, nunca por passagem de tempo.

**Tiers de comida, XP e emoji:**

| Score | Tier | Comida (MVP) | XP |
|---|---|---|---|
| 90–100 | S | 🥩 🍣 | +6 |
| 70–89 | A | 🍜 🌮 | +4 |
| 40–69 | B | 🍕 🍔 | +2 |
| 0–39 | C | 🍩 🍬 | +1 |

**No MVP a comida é emoji renderizado como texto** — zero custo, legibilidade imediata. Sprites próprios em V1.1.

Raridade fica para V1.1: sem base instalada não gera compartilhamento, gera trabalho.

**Nível:** `xp_para_nivel(n) = 50 × n^1.5`, arredondado. Curva a validar em M3 com dados reais de dogfood — deve render subida perceptível na primeira semana sem estagnar no primeiro mês.

### 7.5 Coreografia

```
prompt detectado  → analyzing        (0.6s)
score provisório  → comida cai       → eating
tier da comida    → happy | disappointed
turno fecha       → ajuste sutil     (1s, não bloqueante)
```

O ajuste da fase 2 é uma batida curta — soluço se caiu, brilho rápido se subiu. Sem ele o usuário não percebe que o resultado importou; com cerimônia completa, vira interrupção.

### 7.6 Ritual diário

Disparo: ociosidade detectada ou horário configurável. Card no overlay, ~15 segundos.

1. **Melhor prompt do dia** — o que o fez funcionar
2. **Pior prompt do dia** — o que faltou, concretamente
3. **Delta de fitness** — para onde o ponteiro foi

Um botão: *Entendi*. Pet ganha refeição bônus.

Motivo do desenho: o objetivo declarado é ensinar. Uma escolha genérica de "treinar/descansar" seria mecânica vazia. Reencontrar o próprio pior prompt com o diagnóstico ao lado é o único momento em que aprendizado de fato acontece — voluntário, curto, recompensado.

**Depende de texto persistido.** No modo somente-metadados o ritual degrada para números.

---

## 8. Mascote

### 8.1 MVP — Kuriboh Alado (placeholder)

`~/.codex/pets/kuriboh-alado/` — `spritesheet.webp` 1536×2288 com alpha, ~2.3MB. Grade provável 8 colunas × 11 linhas, célula 192×208 (**confirmar em M1 com visualizador de grade**).

Mapa provisório de estados:

| Linha | Frames | Estado |
|---|---|---|
| 1 | 7 | `idle` + piscar |
| 2–3 | 8 + 8 | voo lateral, duas direções |
| 4–5 | 4 + 5 | flutuar / asas abertas → `analyzing` |
| 6 | 8 | tristeza, sono → `disappointed`, `sleeping` |
| 7 | 6 | surpresa, boca aberta → `notice`, mastigação |
| 8 | 6 | agitação → `celebrating` |
| 9 | 6 | conjunto de expressões |
| 10–11 | 8 + 8 | loops de voo pequeno |

### 8.2 Lacunas resolvidas sem arte nova

**Sem animação de comer** — composição: boca aberta da linha 7 como mastigação + sprite de comida em tween até a boca + bounce de escala + corte para `celebrating`/`disappointed`.

**Sem sprites de comida** — emoji no MVP.

Resultado: **arte fora do caminho crítico**.

### 8.3 Sistema data-driven

```
pets/<pet-id>/
  pet.json          -- id, displayName, versão da folha
  spritesheet.webp
  animations.json   -- estado → { linha, frames, fps, loop }
```

Nenhuma referência a `kuriboh` em código. Trocar mascote = trocar arquivos.

### 8.4 IP — decisão registrada

"Kuriboh Alado" é personagem da Konami (Yu-Gi-Oh!). Uso como placeholder para dogfooding e teste fechado. **Mascote original obrigatório antes de qualquer distribuição pública** (M6). Mascote é o ativo mais visível de um produto e não pode ser IP de terceiro.

---

## 9. Dados e persistência

### 9.1 Esquema (SQLite via `rusqlite`, feature `bundled`)

```sql
source_files (path PK, provider, cursor_bytes, last_seen)
sessions     (id PK, provider, cwd_hash, started_at, last_activity)

prompts (
  id PK, session_id FK, seq, provider, ts,
  prompt_type,          -- initial|followup|continuation|correction|question
  text_hash,            -- sempre presente
  text,                 -- NULL no modo somente-metadados
  structural_score, outcome_score, final_score, confidence,
  dimensions_json, engine_version,
  status                -- provisional|final|partial
)

outcomes (prompt_id FK, clarification, correction_next, rework,
          accepted, tool_calls, duration_ms, aborted)

rewards    (prompt_id FK, food, tier, xp_delta, fitness_delta)
pet_state  (id=1, pet_id, fitness, xp, level, updated_at)
baseline   (id=1, created_at, n_samples, percentiles_json, engine_version)
meta       (key PK, value)
```

`cwd_hash` em vez de caminho: nome de projeto de cliente é sensível e desnecessário além do agrupamento.

### 9.2 Backfill

Primeiro run varre todos os transcripts existentes, pontua e constrói a baseline congelada.

**Backfill não alimenta o pet.** Zero XP, zero fitness. Nascer no nível 40 destrói a única coisa que o produto vende — a progressão. Backfill produz diagnóstico, não atalho.

Onboarding: *"Analisei seus 437 prompts anteriores. Média 61. Seu Promptchi começa aqui."*

---

## 10. Privacidade

### 10.1 Modos

**Padrão: texto armazenado localmente.**

O risco que o usuário teme é egresso, não armazenamento. Os transcripts originais já estão em texto puro no mesmo disco, colocados lá pelas próprias ferramentas — guardar uma cópia local adiciona risco marginal quase nulo e habilita ritual diário, rescore e histórico.

**Modo somente-metadados** em um clique, com consequências declaradas na tela: ritual degrada para números, rescore impossível, histórico sem conteúdo.

Sem SQLCipher. Criptografar um banco cuja fonte está em claro no mesmo disco é teatro de segurança; melhor documentar a realidade.

### 10.2 Egresso — lista fechada

Nada sai da máquina, exceto:

1. **Feedback por LLM** — opt-in, chave do usuário, filtrado, com pré-visualização do que será enviado na primeira vez
2. **Telemetria** — desligada por padrão, opt-in, apenas contadores agregados, nunca texto nem hash de texto, lista documentada

Sem essas duas, o app é 100% offline.

### 10.3 Filtro de secrets

Roda apenas antes de egresso. Detecta prefixos conhecidos (`sk-`, `ghp_`, `AKIA`, `xox`, `-----BEGIN * PRIVATE KEY`), atribuições estilo `.env`, formato JWT, URLs com credencial, strings de alta entropia acima de limiar.

Redação com marcador visível (`[REDACTED:aws_key]`), nunca remoção silenciosa.

### 10.4 Exclusão

Botão que apaga o banco, executa `VACUUM` e remove o arquivo. Sem lixeira, sem retenção oculta. Transcripts originais nunca são tocados.

---

## 11. Milestones

Sequência por retirada de risco, não por feature.

**M0 — Spike de overlay** *(descartável, 1–2 dias)*
Janela Tauri transparente, always-on-top, click-through, kuriboh animado via `steps()`.
*Gate:* funciona sem artefato de fundo. Se falhar, replanejar antes de escrever outra linha.

**M1 — Captura verificável** *(headless)*
Core Rust: watcher, ingest, adapters. Binário de debug que imprime prompts em tempo real. Confirmação da grade do spritesheet.
*Gate:* um dia de trabalho real, zero prompt perdido ou duplicado.

**M2 — Scoring determinístico**
Classificador, heurística, turnos, sinais de resultado, baseline.
*Gate:* 100 prompts rotulados, snapshot verde, score reproduzível.

**M3 — Loop completo**
Progressão, store, overlay em duas fases. Primeira vez que o produto existe.
*Gate:* uma semana de dogfood.

**M4 — Dashboard + ritual diário**
*Gate:* o ritual ensina algo?

**M5 — Empacotamento**
Instalador, tray, autostart, onboarding com backfill, tela de privacidade, exclusão.
*Gate:* instalação em máquina limpa por outra pessoa.

**M6 — Mascote original + beta fechado**

Nenhum polimento antes de M1 passar.

---

## 12. Testes

| Tipo | Cobre |
|---|---|
| Unitário com fixtures | linha parcial, JSON inválido, arquivo truncado, rotação de sessão |
| Golden / snapshot | 100 prompts → scores; mudança sem bump de versão quebra o build |
| Propriedade | pureza (execuções idênticas), faixa 0–100, peso zero nunca penaliza |
| Integração | replay de sessão completa → estado final determinístico |
| Dogfood | única prova de "é divertido" |

Fixtures derivam dos transcripts reais do autor, anonimizados.

---

## 13. Riscos

**1. Transparência de WebView2 falha.** Antecipado por M0. Plano B: janela opaca com cantos arredondados — perde magia, mantém produto.

**2. Formato dos transcripts muda.** Adapters isolados e versionados, com fixtures. Degradação silenciosa obrigatória.

**3. Score "parece errado".** Maior risco de produto. Mitigações: baseline pessoal, explicação sempre visível, e botão **"discordo"** que grava o rótulo — converte usuário em pipeline de calibração e produz o dado necessário para melhorar.

**4. Sinal de resultado ruidoso.** Banda de ±20, confiança gravada, ajuste oculto sob baixa confiança.

**5. Não é divertido.** Critério de parada definido: se após duas semanas de uso o autor não sentir falta ao fechar, o problema é o conceito, não a execução.

**6. RAM incomoda público dev.** Medir em M3. Render suspenso em idle; opção de viver só no tray.

**7. IP do mascote.** Resolvido por M6.

---

## 14. Complexidade relativa

| Parte | Esforço |
|---|---|
| Scoring engine | Alto — é o coração |
| Watcher + ingest + adapters | Médio |
| Overlay + animação | Médio |
| Dashboard | Médio |
| Empacotamento + onboarding | Médio (sempre subestimado) |
| Progressão + store | Baixo |
| Ritual diário | Baixo |

---

## 15. Escopo

### 15.1 No MVP

Captura Claude Code + Codex · classificação de tipo · scoring em duas fases · baseline congelada · fitness e XP · overlay com 5 estados · comida em 4 tiers · ritual diário · dashboard mínimo · backfill · modos de privacidade · exclusão de dados.

### 15.2 Removido em definitivo

- **Intelligence, Happiness, Energy** — não mensuráveis a partir do sinal disponível
- **Captura via web/DOM** — arquitetura descartada
- **Marketplace, batalha, leaderboard global, mobile**

### 15.3 Adiado

| Versão | Itens |
|---|---|
| V1.1 | streak (por qualidade), achievements, raridade, desafios diários, sprites de comida, estágios corporais |
| V1.2 | personalidade, humor, acessórios |
| V1.3 | evoluções especializadas, espécies, ovos |
| V2 | novos providers, social, sync opcional |

---

## 16. Métricas

**Ativação:** instalou → backfill concluído → primeiro prompt avaliado ao vivo → primeira comida.

**Valor:** `média(primeiros 20)` vs `média(últimos 20)` contra a baseline congelada. Métrica principal do produto.

**Engajamento:** prompts avaliados/dia · ritual diário concluído · abertura do dashboard.

**Retenção:** D1, D7, D30.

Todas calculáveis localmente. Envio só com telemetria opt-in.
