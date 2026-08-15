# Promptchi M0+M1 — Fundação Verificável: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provar que o overlay transparente funciona e que a captura de prompts é confiável, antes de escrever qualquer lógica de produto.

**Architecture:** Duas entregas independentes. **M0** é um spike descartável em Tauri que valida janela transparente, always-on-top, click-through e animação de sprite. **M1** é o núcleo Rust puro de captura — divisão de linhas JSONL com buffer de linha parcial, adapters por provider, e cursor por arquivo — mais um binário de debug que imprime prompts detectados em tempo real. Nenhuma lógica de score, nenhum pet, nenhum banco.

**Tech Stack:** Rust 1.97.1, cargo workspace, `serde_json`, `notify`, `anyhow`. Tauri v2 apenas no spike de M0. Node 24.14.1 disponível.

## Global Constraints

- `core/` é crate **puro**: sem Tauri, sem I/O de rede, sem dependência de SO. Somente `serde` e `serde_json`.
- O app **apenas lê** transcripts. Nunca escreve, move, renomeia ou apaga arquivos em `~/.claude` ou `~/.codex`.
- Formato desconhecido ou JSON inválido **degrada em silêncio** — retorna `None` e segue. Nunca `panic!`, nunca `unwrap()` em dado externo.
- Nenhuma regra de negócio em JavaScript.
- Caminhos de transcript: `~/.claude/projects/<slug>/<sessionId>.jsonl` e `~/.codex/sessions/**/*.jsonl` + `~/.codex/archived_sessions/*.jsonl`.
- Todo commit usa mensagem em português, prefixo convencional (`feat:`, `test:`, `chore:`, `docs:`).

---

## File Structure

```
promptchi/
  Cargo.toml                        workspace
  core/
    Cargo.toml
    src/
      lib.rs                        re-exporta módulos públicos
      types.rs                      Provider, PromptEvent
      jsonl.rs                      LineBuffer — divisão com linha parcial
      cursor.rs                     FileCursor — leitura incremental por offset
      adapters/
        mod.rs                      trait PromptSource + registry
        claude_code.rs              ClaudeCodeAdapter
        codex.rs                    CodexAdapter
    tests/
      fixtures/
        claude_code.jsonl           amostra anonimizada
        codex.jsonl                 amostra anonimizada
      integration.rs                replay de fixture completa
  cli/
    Cargo.toml
    src/main.rs                     binário de debug: watch + print
  spike-overlay/                    M0, descartável
```

Separação por responsabilidade: `jsonl` não sabe o que é um prompt; `adapters` não sabem ler arquivos; `cursor` não sabe interpretar conteúdo. Cada um é testável isolado.

---

# M0 — Spike de Overlay (descartável)

Sem testes automatizados. Transparência de WebView2 sobre desktop vivo só se verifica com olho humano. O gate é visual e é eliminatório.

### Task 1: Janela transparente always-on-top

**Files:**
- Create: `spike-overlay/` (scaffold Tauri v2)
- Modify: `spike-overlay/src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: nada
- Produces: veredito go/no-go sobre a viabilidade do overlay

- [ ] **Step 1: Criar scaffold Tauri**

```bash
cd C:/Users/evolu/Documents/freela/promptchi
npm create tauri-app@latest spike-overlay -- --template vanilla --manager npm --yes
cd spike-overlay && npm install
```

- [ ] **Step 2: Configurar a janela como overlay**

Em `spike-overlay/src-tauri/tauri.conf.json`, substituir o objeto de janela em `app.windows[0]`:

```json
{
  "label": "pet",
  "width": 240,
  "height": 240,
  "transparent": true,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "shadow": false,
  "resizable": false,
  "center": false,
  "x": 1500,
  "y": 800
}
```

- [ ] **Step 3: Tornar o fundo transparente em toda a cadeia CSS**

Substituir `spike-overlay/src/styles.css` inteiro por:

```css
html, body {
  margin: 0;
  padding: 0;
  background: transparent !important;
  overflow: hidden;
  height: 100%;
}
#app { background: transparent; }
#probe {
  width: 120px; height: 120px;
  margin: 60px;
  background: crimson;
  border-radius: 50%;
}
```

Substituir o conteúdo de `<body>` em `spike-overlay/index.html` por:

```html
<div id="app"><div id="probe"></div></div>
```

- [ ] **Step 4: Rodar e verificar visualmente**

```bash
npm run tauri dev
```

Verificar, um a um:
1. Só o círculo vermelho aparece — nenhum retângulo branco, cinza ou preto ao redor
2. A janela fica sobre o VS Code e sobre o navegador
3. A janela não aparece na barra de tarefas
4. A janela não aparece no Alt+Tab

**Se qualquer fundo sólido aparecer ao redor do círculo, PARE.** Registrar o resultado, o modelo da GPU e a versão do WebView2, e acionar o Plano B do spec (janela opaca com cantos arredondados) antes de continuar.

- [ ] **Step 5: Commit**

```bash
git add spike-overlay
git commit -m "chore: spike de overlay transparente em Tauri"
```

---

### Task 2: Sprite animado e click-through

**Files:**
- Create: `spike-overlay/src/kuriboh.webp` (cópia do asset)
- Modify: `spike-overlay/src/styles.css`, `spike-overlay/index.html`
- Modify: `spike-overlay/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: janela validada na Task 1
- Produces: grade confirmada do spritesheet (colunas, linhas, célula), registrada em `docs/superpowers/notes/spritesheet-grid.md`

- [ ] **Step 1: Copiar o asset**

```bash
cp "C:/Users/evolu/.codex/pets/kuriboh-alado/spritesheet.webp" spike-overlay/src/kuriboh.webp
```

- [ ] **Step 2: Renderizar a folha inteira com grade sobreposta**

Adicionar em `spike-overlay/src/styles.css`:

```css
#grid {
  width: 1536px; height: 2288px;
  background-image: url("./kuriboh.webp");
  background-size: 1536px 2288px;
  position: absolute; top: 0; left: 0;
  transform: scale(0.35); transform-origin: top left;
}
#grid::after {
  content: ""; position: absolute; inset: 0;
  background-image:
    repeating-linear-gradient(to right,  rgba(255,0,0,.9) 0 2px, transparent 2px 192px),
    repeating-linear-gradient(to bottom, rgba(0,128,255,.9) 0 2px, transparent 2px 208px);
}
```

Trocar o conteúdo de `#app` para `<div id="grid"></div>` e ajustar a janela para `1000x900`, `transparent: false` temporariamente.

- [ ] **Step 3: Confirmar a grade visualmente**

```bash
npm run tauri dev
```

Verificar se as linhas caem nas bordas dos frames. Se não caírem, ajustar os valores `192px` e `208px` até encaixar.

Registrar o resultado em `docs/superpowers/notes/spritesheet-grid.md`:

```markdown
# Grade do spritesheet — kuriboh-alado

- Imagem: 1536 x 2288
- Célula: <largura> x <altura>
- Colunas: <n>   Linhas: <n>
- Frames por linha (contagem real, esquerda para direita):
  linha 1: <n>  ...
```

- [ ] **Step 4: Animar uma linha com `steps()`**

Reverter a janela para `240x240` e `transparent: true`. Substituir `#grid` por:

```css
#pet {
  width: 192px; height: 208px;
  margin: 16px auto;
  background-image: url("./kuriboh.webp");
  background-repeat: no-repeat;
  background-position: 0 0;
  animation: idle 1.05s steps(7) infinite;
}
@keyframes idle {
  from { background-position:     0px 0px; }
  to   { background-position: -1344px 0px; }
}
```

`-1344px` = 7 frames × 192px. Ajustar se a Task 2 Step 3 apurou valores diferentes.

- [ ] **Step 5: Verificar a animação sobre o desktop**

```bash
npm run tauri dev
```

Verificar: o kuriboh anima em loop, sem fundo sólido, sem costura visível entre o último frame e o primeiro.

- [ ] **Step 6: Ativar click-through**

Em `spike-overlay/src-tauri/src/lib.rs`, dentro do `setup`:

```rust
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let w = app.get_webview_window("pet").unwrap();
            w.set_ignore_cursor_events(true)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar spike");
}
```

- [ ] **Step 7: Verificar click-through**

```bash
npm run tauri dev
```

Posicionar a janela sobre um botão do VS Code e clicar através dela. O clique deve chegar ao VS Code.

**Gate de M0:** transparência, always-on-top, animação e click-through funcionando juntos. Se sim, M0 está aposentado e o design do spec está validado.

- [ ] **Step 8: Commit**

```bash
git add spike-overlay docs/superpowers/notes/spritesheet-grid.md
git commit -m "chore: spike valida sprite animado e click-through"
```

---

# M1 — Núcleo de Captura

A partir daqui tudo é TDD. O núcleo é lógica pura e determinística.

### Task 3: Workspace e tipos de domínio

**Files:**
- Create: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/types.rs`

**Interfaces:**
- Consumes: nada
- Produces: `Provider` (enum: `ClaudeCode`, `Codex`), `PromptEvent { provider: Provider, session_id: String, text: String, timestamp: String, cwd: Option<String> }`

- [ ] **Step 1: Criar o workspace**

`Cargo.toml` na raiz:

```toml
[workspace]
resolver = "2"
members = ["core"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
notify = "6"
```

`cli` entra em `members` só na Task 9, quando o crate passa a existir. Listar um membro inexistente quebra todo `cargo` no workspace.

`core/Cargo.toml`:

```toml
[package]
name = "promptchi-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
```

`core/src/lib.rs` — precisa existir antes do primeiro `cargo test`, senão o erro será "couldn't find lib.rs" em vez do erro de tipo esperado:

```rust
pub mod types;
```

- [ ] **Step 2: Escrever o teste que falha**

`core/src/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_tem_nome_estavel() {
        assert_eq!(Provider::ClaudeCode.as_str(), "claude_code");
        assert_eq!(Provider::Codex.as_str(), "codex");
    }
}
```

- [ ] **Step 3: Rodar o teste e confirmar que falha**

```bash
cargo test -p promptchi-core
```

Esperado: erro de compilação, `cannot find type Provider`.

- [ ] **Step 4: Implementar os tipos**

No topo de `core/src/types.rs`, acima do bloco `mod tests`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    ClaudeCode,
    Codex,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::ClaudeCode => "claude_code",
            Provider::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptEvent {
    pub provider: Provider,
    pub session_id: String,
    pub text: String,
    pub timestamp: String,
    pub cwd: Option<String>,
}
```

Atualizar `core/src/lib.rs` para reexportar os tipos:

```rust
pub mod types;
pub use types::{PromptEvent, Provider};
```

- [ ] **Step 5: Rodar o teste e confirmar que passa**

```bash
cargo test -p promptchi-core
```

Esperado: `test result: ok. 1 passed`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml core
git commit -m "feat: workspace e tipos de domínio do core"
```

---

### Task 4: Divisão de JSONL com buffer de linha parcial

Escrita de JSONL não é atômica. Ler durante uma escrita entrega linha cortada ao meio. Este módulo garante que só linhas completas sejam emitidas.

**Files:**
- Create: `core/src/jsonl.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: nada
- Produces: `LineBuffer::new() -> LineBuffer`, `LineBuffer::push(&mut self, chunk: &str) -> Vec<String>`, `LineBuffer::clear(&mut self)`

- [ ] **Step 1: Escrever os testes que falham**

`core/src/jsonl.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linha_incompleta_nao_e_emitida() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("{\"a\":1"), Vec::<String>::new());
    }

    #[test]
    fn linha_completa_no_chunk_seguinte() {
        let mut b = LineBuffer::new();
        b.push("{\"a\":1");
        assert_eq!(b.push("}\n"), vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn multiplas_linhas_em_um_chunk() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("um\ndois\ntres"), vec!["um".to_string(), "dois".to_string()]);
    }

    #[test]
    fn crlf_e_removido() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("um\r\n"), vec!["um".to_string()]);
    }

    #[test]
    fn linhas_vazias_sao_ignoradas() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("\n\num\n"), vec!["um".to_string()]);
    }

    #[test]
    fn utf8_multibyte_nao_quebra() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("olá çãé\n"), vec!["olá çãé".to_string()]);
    }

    #[test]
    fn clear_descarta_pendente() {
        let mut b = LineBuffer::new();
        b.push("parcial");
        b.clear();
        assert_eq!(b.push("completa\n"), vec!["completa".to_string()]);
    }
}
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core jsonl
```

Esperado: erro de compilação, `cannot find type LineBuffer`.

- [ ] **Step 3: Implementar**

No topo de `core/src/jsonl.rs`:

```rust
/// Acumula bytes lidos e emite apenas linhas terminadas em `\n`.
/// A linha final incompleta fica retida até o próximo `push`.
#[derive(Debug, Default)]
pub struct LineBuffer {
    pending: String,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self { pending: String::new() }
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);
        let mut out = Vec::new();
        while let Some(idx) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=idx).collect();
            let trimmed = line.trim_end_matches(['\n', '\r']);
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}
```

Adicionar em `core/src/lib.rs`:

```rust
pub mod jsonl;
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core jsonl
```

Esperado: `7 passed`.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "feat: buffer de linha parcial para leitura de JSONL"
```

---

### Task 5: Trait PromptSource e adapter do Claude Code

O tipo `user` do Claude Code cobre tanto prompt humano quanto `tool_result` injetado pelo sistema. A discriminação é pela forma do `content`.

**Files:**
- Create: `core/src/adapters/mod.rs`, `core/src/adapters/claude_code.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: `PromptEvent`, `Provider` da Task 3
- Produces: `trait PromptSource { fn provider(&self) -> Provider; fn parse_line(&self, line: &str, session_id: &str) -> Option<PromptEvent>; }` e `ClaudeCodeAdapter`

- [ ] **Step 1: Escrever os testes que falham**

`core/src/adapters/claude_code.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::PromptSource;

    fn a() -> ClaudeCodeAdapter { ClaudeCodeAdapter }

    #[test]
    fn content_string_e_prompt_humano() {
        let l = r#"{"type":"user","message":{"role":"user","content":"corrige o null check"},"timestamp":"2026-07-15T13:00:00.000Z","cwd":"C:\\proj"}"#;
        let e = a().parse_line(l, "s1").expect("deveria capturar");
        assert_eq!(e.text, "corrige o null check");
        assert_eq!(e.session_id, "s1");
        assert_eq!(e.provider, Provider::ClaudeCode);
        assert_eq!(e.cwd.as_deref(), Some("C:\\proj"));
    }

    #[test]
    fn tool_result_e_ignorado() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn sidechain_e_ignorado() {
        let l = r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"do subagente"},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn is_meta_e_ignorado() {
        let l = r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn marcador_de_interrupcao_e_ignorado() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn array_apenas_com_text_e_aceito() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"primeira"},{"type":"text","text":"segunda"}]},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        let e = a().parse_line(l, "s1").expect("deveria capturar");
        assert_eq!(e.text, "primeira\nsegunda");
    }

    #[test]
    fn assistant_e_ignorado() {
        let l = r#"{"type":"assistant","message":{"role":"assistant","content":"resposta"},"timestamp":"2026-07-15T13:00:00.000Z"}"#;
        assert!(a().parse_line(l, "s1").is_none());
    }

    #[test]
    fn json_invalido_nao_causa_panic() {
        assert!(a().parse_line("{quebrado", "s1").is_none());
        assert!(a().parse_line("", "s1").is_none());
    }
}
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core claude_code
```

Esperado: erro de compilação, `cannot find type ClaudeCodeAdapter`.

- [ ] **Step 3: Definir a trait**

`core/src/adapters/mod.rs`:

```rust
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
```

- [ ] **Step 4: Implementar o adapter**

No topo de `core/src/adapters/claude_code.rs`:

```rust
use serde_json::Value;

use crate::adapters::PromptSource;
use crate::types::{PromptEvent, Provider};

const MARCADORES_DE_SISTEMA: [&str; 2] = [
    "[Request interrupted by user]",
    "[Request interrupted by user for tool use]",
];

pub struct ClaudeCodeAdapter;

impl PromptSource for ClaudeCodeAdapter {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn parse_line(&self, line: &str, session_id: &str) -> Option<PromptEvent> {
        let v: Value = serde_json::from_str(line).ok()?;

        if v.get("type")?.as_str()? != "user" {
            return None;
        }
        if v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }
        if v.get("isMeta").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }

        let message = v.get("message")?;
        if message.get("role")?.as_str()? != "user" {
            return None;
        }

        let text = extrair_texto_humano(message.get("content")?)?;
        if MARCADORES_DE_SISTEMA.contains(&text.as_str()) {
            return None;
        }

        Some(PromptEvent {
            provider: Provider::ClaudeCode,
            session_id: session_id.to_string(),
            text,
            timestamp: v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string(),
            cwd: v.get("cwd").and_then(Value::as_str).map(str::to_string),
        })
    }
}

/// `content` string = prompt digitado.
/// `content` array = aceito somente se não contiver nenhum bloco `tool_result`.
fn extrair_texto_humano(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        }
        Value::Array(blocos) => {
            let tem_tool_result = blocos
                .iter()
                .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"));
            if tem_tool_result {
                return None;
            }
            let partes: Vec<&str> = blocos
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if partes.is_empty() { None } else { Some(partes.join("\n")) }
        }
        _ => None,
    }
}
```

Adicionar em `core/src/lib.rs`:

```rust
pub mod adapters;
```

- [ ] **Step 5: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core claude_code
```

Esperado: `8 passed`.

- [ ] **Step 6: Commit**

```bash
git add core
git commit -m "feat: adapter de captura do Claude Code"
```

---

### Task 6: Adapter do Codex

Formato direto, sem ambiguidade: `type:"event_msg"` com `payload.type:"user_message"`.

**Files:**
- Create: `core/src/adapters/codex.rs`
- Modify: `core/src/adapters/mod.rs` (adicionar `pub mod codex;`)

**Interfaces:**
- Consumes: `PromptSource` da Task 5
- Produces: `CodexAdapter`, `sessao_do_caminho(path: &str) -> Option<String>`

- [ ] **Step 1: Escrever os testes que falham**

`core/src/adapters/codex.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::PromptSource;

    fn a() -> CodexAdapter { CodexAdapter }

    #[test]
    fn user_message_e_capturado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:40.258Z","type":"event_msg","payload":{"type":"user_message","client_id":"abc","message":"muda a sequencia de refugo\n","images":[]}}"#;
        let e = a().parse_line(l, "s9").expect("deveria capturar");
        assert_eq!(e.text, "muda a sequencia de refugo");
        assert_eq!(e.provider, Provider::Codex);
        assert_eq!(e.timestamp, "2026-06-24T15:52:40.258Z");
    }

    #[test]
    fn agent_message_e_ignorado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:41.000Z","type":"event_msg","payload":{"type":"agent_message","message":"resposta"}}"#;
        assert!(a().parse_line(l, "s9").is_none());
    }

    #[test]
    fn response_item_e_ignorado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:41.000Z","type":"response_item","payload":{"type":"function_call","name":"shell"}}"#;
        assert!(a().parse_line(l, "s9").is_none());
    }

    #[test]
    fn session_meta_e_ignorado() {
        let l = r#"{"timestamp":"2026-06-24T15:52:40.182Z","type":"session_meta","payload":{"session_id":"abc","cwd":"C:\\proj"}}"#;
        assert!(a().parse_line(l, "s9").is_none());
    }

    #[test]
    fn json_invalido_nao_causa_panic() {
        assert!(a().parse_line("{quebrado", "s9").is_none());
    }

    #[test]
    fn extrai_sessao_do_nome_do_arquivo() {
        let p = "C:/Users/x/.codex/archived_sessions/rollout-2026-06-24T12-52-17-019efa54-f763-7691-91d3-c9a38153c864.jsonl";
        assert_eq!(
            sessao_do_caminho(p).as_deref(),
            Some("019efa54-f763-7691-91d3-c9a38153c864")
        );
    }

    #[test]
    fn nome_sem_uuid_retorna_none() {
        assert_eq!(sessao_do_caminho("C:/x/qualquer.jsonl"), None);
    }
}
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core codex
```

Esperado: erro de compilação, `cannot find type CodexAdapter`.

- [ ] **Step 3: Implementar**

Primeiro, adicionar a declaração do módulo em `core/src/adapters/mod.rs`, logo abaixo de `pub mod claude_code;`:

```rust
pub mod codex;
```

Depois, no topo de `core/src/adapters/codex.rs`:

```rust
use serde_json::Value;

use crate::adapters::PromptSource;
use crate::types::{PromptEvent, Provider};

pub struct CodexAdapter;

impl PromptSource for CodexAdapter {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn parse_line(&self, line: &str, session_id: &str) -> Option<PromptEvent> {
        let v: Value = serde_json::from_str(line).ok()?;

        if v.get("type")?.as_str()? != "event_msg" {
            return None;
        }
        let payload = v.get("payload")?;
        if payload.get("type")?.as_str()? != "user_message" {
            return None;
        }

        let texto = payload.get("message")?.as_str()?.trim();
        if texto.is_empty() {
            return None;
        }

        Some(PromptEvent {
            provider: Provider::Codex,
            session_id: session_id.to_string(),
            text: texto.to_string(),
            timestamp: v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string(),
            cwd: None,
        })
    }
}

/// Extrai o UUID de sessão de `rollout-<timestamp>-<uuid>.jsonl`.
/// O UUID são os últimos 5 segmentos separados por hífen, no formato 8-4-4-4-12.
pub fn sessao_do_caminho(path: &str) -> Option<String> {
    let arquivo = path.rsplit(['/', '\\']).next()?;
    let base = arquivo.strip_suffix(".jsonl")?;
    let partes: Vec<&str> = base.split('-').collect();
    if partes.len() < 5 {
        return None;
    }
    let uuid: Vec<&str> = partes[partes.len() - 5..].to_vec();
    let tamanhos = [8usize, 4, 4, 4, 12];
    for (parte, esperado) in uuid.iter().zip(tamanhos.iter()) {
        if parte.len() != *esperado || !parte.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
    }
    Some(uuid.join("-"))
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core codex
```

Esperado: `7 passed`.

- [ ] **Step 5: Commit**

```bash
git add core
git commit -m "feat: adapter de captura do Codex"
```

---

### Task 7: Cursor de leitura incremental

Guarda o offset por arquivo e lê apenas o que chegou desde a última leitura. Trata truncamento e substituição de arquivo.

**Files:**
- Create: `core/src/cursor.rs`
- Modify: `core/src/lib.rs`, `core/Cargo.toml`

**Interfaces:**
- Consumes: `LineBuffer` da Task 4
- Produces: `FileCursor::new(path: PathBuf) -> FileCursor`, `FileCursor::read_new(&mut self) -> std::io::Result<Vec<String>>`, `FileCursor::offset(&self) -> u64`

- [ ] **Step 1: Adicionar a dependência de teste**

Em `core/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Escrever os testes que falham**

`core/src/cursor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    fn escrever(path: &std::path::Path, conteudo: &str) {
        let mut f = OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(conteudo.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    #[test]
    fn le_apenas_o_que_chegou_desde_a_ultima_leitura() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "um\ndois\n");

        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), vec!["um".to_string(), "dois".to_string()]);
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        escrever(&p, "tres\n");
        assert_eq!(c.read_new().unwrap(), vec!["tres".to_string()]);
    }

    #[test]
    fn linha_parcial_e_retida_ate_completar() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "{\"a\":");

        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        escrever(&p, "1}\n");
        assert_eq!(c.read_new().unwrap(), vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn arquivo_truncado_reinicia_do_zero() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "antigo\n");

        let mut c = FileCursor::new(p.clone());
        c.read_new().unwrap();
        assert!(c.offset() > 0);

        std::fs::write(&p, "novo\n").unwrap();
        assert_eq!(c.read_new().unwrap(), vec!["novo".to_string()]);
    }

    #[test]
    fn arquivo_inexistente_retorna_vazio_sem_erro() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = FileCursor::new(dir.path().join("nao_existe.jsonl"));
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());
    }
}
```

- [ ] **Step 3: Rodar os testes e confirmar que falham**

```bash
cargo test -p promptchi-core cursor
```

Esperado: erro de compilação, `cannot find type FileCursor`.

- [ ] **Step 4: Implementar**

No topo de `core/src/cursor.rs`:

```rust
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::jsonl::LineBuffer;

/// Quantidade de bytes iniciais usada como "impressão digital" do arquivo,
/// para detectar substituição sem depender de metadados específicos de SO.
const TAMANHO_PREFIXO: usize = 32;

/// Lê incrementalmente um arquivo append-only, emitindo apenas linhas completas.
pub struct FileCursor {
    path: PathBuf,
    offset: u64,
    buffer: LineBuffer,
    prefixo: Vec<u8>,
}

impl FileCursor {
    pub fn new(path: PathBuf) -> Self {
        Self { path, offset: 0, buffer: LineBuffer::new(), prefixo: Vec::new() }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn read_new(&mut self) -> std::io::Result<Vec<String>> {
        // O arquivo pode sumir a qualquer momento nesta função (corrida com o
        // processo escritor). Em todos os pontos de I/O, `NotFound` degrada em
        // silêncio para lista vazia; qualquer outro erro propaga.
        macro_rules! ou_vazio {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(Vec::new())
                    }
                    Err(e) => return Err(e),
                }
            };
        }

        let mut f = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        // Impressão digital do início do arquivo: se os bytes iniciais mudaram,
        // o arquivo foi substituído por outro — mesmo com tamanho igual ou
        // maior, caso que o encolhimento sozinho não detecta. Comparar apenas o
        // prefixo comum evita falso positivo em arquivo que só cresceu (bytes
        // iniciais continuam iguais) e em arquivo vazio (n = 0).
        ou_vazio!(f.seek(SeekFrom::Start(0)));
        let mut atual = Vec::new();
        ou_vazio!((&mut f).take(TAMANHO_PREFIXO as u64).read_to_end(&mut atual));
        let n = self.prefixo.len().min(atual.len());
        if self.prefixo[..n] != atual[..n] {
            self.offset = 0;
            self.buffer.clear();
        }
        self.prefixo = atual;

        let tamanho = ou_vazio!(f.metadata()).len();

        // Arquivo encolheu: foi truncado. Recomeçar.
        if tamanho < self.offset {
            self.offset = 0;
            self.buffer.clear();
        }
        if tamanho == self.offset {
            return Ok(Vec::new());
        }

        ou_vazio!(f.seek(SeekFrom::Start(self.offset)));
        let mut bytes = Vec::new();
        let lidos = ou_vazio!(f.read_to_end(&mut bytes)) as u64;
        self.offset += lidos;

        // Bytes inválidos não devem derrubar a captura.
        let texto = String::from_utf8_lossy(&bytes);
        Ok(self.buffer.push(&texto))
    }
}
```

> **Correção aplicada durante a execução.** A primeira versão deste plano
> detectava substituição de arquivo apenas por encolhimento
> (`tamanho < self.offset`). Isso era um defeito: substituição por arquivo de
> tamanho igual ou maior fazia o `seek` ir para o offset antigo dentro do
> arquivo novo, concatenando a linha parcial retida com bytes de outro arquivo
> e emitindo uma linha fabricada, além de perder o início do arquivo novo em
> silêncio. A impressão digital de prefixo acima resolve o caso. Limitação
> residual aceita: substituição por arquivo cujos 32 primeiros bytes sejam
> idênticos não é detectada — desprezível em JSONL, onde cada linha carrega
> timestamp e identificadores únicos.
>
> Dois testes adicionais cobrem a regressão: substituição por arquivo **maior**
> e por arquivo de **mesmo tamanho**, ambos assertando igualdade estrita ao
> resultado esperado — o que prova a ausência da linha fabricada, não apenas a
> presença da linha correta.

Adicionar em `core/src/lib.rs`:

```rust
pub mod cursor;
```

- [ ] **Step 5: Rodar os testes e confirmar que passam**

```bash
cargo test -p promptchi-core cursor
```

Esperado: `4 passed`.

- [ ] **Step 6: Commit**

```bash
git add core
git commit -m "feat: cursor de leitura incremental por offset"
```

---

### Task 8: Teste de integração com fixtures reais

Prova que os módulos funcionam juntos sobre dados de verdade, e congela o comportamento contra regressão.

**Files:**
- Create: `core/tests/fixtures/claude_code.jsonl`, `core/tests/fixtures/codex.jsonl`, `core/tests/integration.rs`

**Interfaces:**
- Consumes: `FileCursor`, `ClaudeCodeAdapter`, `CodexAdapter`
- Produces: nada — é o gate de qualidade de M1

- [ ] **Step 1: Montar a fixture do Claude Code**

Criar `core/tests/fixtures/claude_code.jsonl` com exatamente estas 6 linhas (uma por linha, sem quebra interna):

```
{"type":"user","message":{"role":"user","content":"cria o endpoint de listagem"},"timestamp":"2026-07-15T13:00:00.000Z","cwd":"C:\\proj","isSidechain":false}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Read"}]},"timestamp":"2026-07-15T13:00:01.000Z"}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"conteudo do arquivo"}]},"timestamp":"2026-07-15T13:00:02.000Z"}
{"type":"user","isSidechain":true,"message":{"role":"user","content":"tarefa do subagente"},"timestamp":"2026-07-15T13:00:03.000Z"}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"timestamp":"2026-07-15T13:00:04.000Z"}
{"type":"user","message":{"role":"user","content":"agora adiciona paginacao"},"timestamp":"2026-07-15T13:00:05.000Z","cwd":"C:\\proj","isSidechain":false}
```

- [ ] **Step 2: Montar a fixture do Codex**

Criar `core/tests/fixtures/codex.jsonl` com exatamente estas 4 linhas:

```
{"timestamp":"2026-06-24T15:52:40.182Z","type":"session_meta","payload":{"session_id":"019efa54-f763-7691-91d3-c9a38153c864","cwd":"C:\\proj"}}
{"timestamp":"2026-06-24T15:52:40.258Z","type":"event_msg","payload":{"type":"user_message","client_id":"abc","message":"o worker de refugo esta ignorando os ciclos\n","images":[]}}
{"timestamp":"2026-06-24T15:53:00.000Z","type":"event_msg","payload":{"type":"agent_message","message":"vou verificar"}}
{"timestamp":"2026-06-24T15:54:57.165Z","type":"event_msg","payload":{"type":"user_message","client_id":"def","message":"so dispara se todos forem refugo\n","images":[]}}
```

- [ ] **Step 3: Escrever o teste de integração**

`core/tests/integration.rs`:

```rust
use std::path::PathBuf;

use promptchi_core::adapters::claude_code::ClaudeCodeAdapter;
use promptchi_core::adapters::codex::{sessao_do_caminho, CodexAdapter};
use promptchi_core::adapters::PromptSource;
use promptchi_core::cursor::FileCursor;
use promptchi_core::types::Provider;

fn fixture(nome: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(nome)
}

#[test]
fn replay_do_claude_code_captura_apenas_prompts_humanos() {
    let mut cursor = FileCursor::new(fixture("claude_code.jsonl"));
    let adapter = ClaudeCodeAdapter;

    let eventos: Vec<_> = cursor
        .read_new()
        .unwrap()
        .iter()
        .filter_map(|l| adapter.parse_line(l, "sessao-teste"))
        .collect();

    assert_eq!(eventos.len(), 2, "esperado 2 prompts humanos de 6 linhas");
    assert_eq!(eventos[0].text, "cria o endpoint de listagem");
    assert_eq!(eventos[1].text, "agora adiciona paginacao");
    assert!(eventos.iter().all(|e| e.provider == Provider::ClaudeCode));
    assert_eq!(eventos[0].cwd.as_deref(), Some("C:\\proj"));
}

#[test]
fn replay_do_codex_captura_apenas_prompts_humanos() {
    let mut cursor = FileCursor::new(fixture("codex.jsonl"));
    let adapter = CodexAdapter;

    let eventos: Vec<_> = cursor
        .read_new()
        .unwrap()
        .iter()
        .filter_map(|l| adapter.parse_line(l, "sessao-teste"))
        .collect();

    assert_eq!(eventos.len(), 2, "esperado 2 prompts humanos de 4 linhas");
    assert_eq!(eventos[0].text, "o worker de refugo esta ignorando os ciclos");
    assert_eq!(eventos[1].text, "so dispara se todos forem refugo");
}

#[test]
fn segunda_leitura_nao_reprocessa_nada() {
    let mut cursor = FileCursor::new(fixture("claude_code.jsonl"));
    assert_eq!(cursor.read_new().unwrap().len(), 6);
    assert_eq!(cursor.read_new().unwrap().len(), 0, "duplicação de prompts");
}

#[test]
fn sessao_do_codex_vem_do_nome_do_arquivo() {
    assert_eq!(
        sessao_do_caminho("x/rollout-2026-06-24T12-52-17-019efa54-f763-7691-91d3-c9a38153c864.jsonl").as_deref(),
        Some("019efa54-f763-7691-91d3-c9a38153c864")
    );
}
```

- [ ] **Step 4: Rodar e confirmar que passa**

```bash
cargo test -p promptchi-core --test integration
```

Esperado: `4 passed`.

- [ ] **Step 5: Rodar a suíte inteira**

```bash
cargo test
```

Esperado: todos os testes de `types`, `jsonl`, `claude_code`, `codex`, `cursor` e `integration` verdes.

- [ ] **Step 6: Commit**

```bash
git add core
git commit -m "test: integração dos adapters com fixtures reais"
```

---

### Task 9: Binário de debug com watcher

O gate de M1: rodar um dia inteiro de trabalho real sem perder nem duplicar prompt.

**Files:**
- Create: `cli/Cargo.toml`, `cli/src/main.rs`
- Modify: `Cargo.toml` (adicionar `cli` a `members`)

**Interfaces:**
- Consumes: todo o `promptchi-core`
- Produces: binário `promptchi-watch`

- [ ] **Step 1: Criar o crate**

Primeiro, alterar `members` no `Cargo.toml` da raiz:

```toml
members = ["core", "cli"]
```

`cli/Cargo.toml`:

```toml
[package]
name = "promptchi-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "promptchi-watch"
path = "src/main.rs"

[dependencies]
promptchi-core = { path = "../core" }
notify = { workspace = true }
anyhow = { workspace = true }
```

- [ ] **Step 2: Implementar o watcher**

`cli/src/main.rs`:

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use promptchi_core::adapters::claude_code::ClaudeCodeAdapter;
use promptchi_core::adapters::codex::{sessao_do_caminho, CodexAdapter};
use promptchi_core::adapters::PromptSource;
use promptchi_core::cursor::FileCursor;
use promptchi_core::types::Provider;

fn raiz_claude() -> Option<PathBuf> {
    dirs_home().map(|h| h.join(".claude").join("projects"))
}

fn raizes_codex() -> Vec<PathBuf> {
    match dirs_home() {
        Some(h) => vec![
            h.join(".codex").join("sessions"),
            h.join(".codex").join("archived_sessions"),
        ],
        None => Vec::new(),
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Deriva o provider e o id de sessão a partir do caminho do arquivo.
fn identificar(path: &Path) -> Option<(Provider, String)> {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.contains("/.claude/projects/") {
        let sessao = path.file_stem()?.to_string_lossy().to_string();
        return Some((Provider::ClaudeCode, sessao));
    }
    if s.contains("/.codex/") {
        let sessao = sessao_do_caminho(&s)
            .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string());
        return Some((Provider::Codex, sessao));
    }
    None
}

fn main() -> anyhow::Result<()> {
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(
        tx,
        Config::default().with_poll_interval(Duration::from_secs(2)),
    )?;

    let mut raizes: Vec<PathBuf> = raizes_codex();
    if let Some(c) = raiz_claude() {
        raizes.push(c);
    }
    for raiz in &raizes {
        if raiz.exists() {
            watcher.watch(raiz, RecursiveMode::Recursive)?;
            println!("observando: {}", raiz.display());
        } else {
            eprintln!("aviso: raiz ausente, ignorada: {}", raiz.display());
        }
    }

    let claude = ClaudeCodeAdapter;
    let codex = CodexAdapter;
    let mut cursores: HashMap<PathBuf, FileCursor> = HashMap::new();
    let mut total = 0usize;

    // Só prompts novos: posiciona os cursores no fim dos arquivos existentes.
    for raiz in &raizes {
        for path in jsonls_em(raiz) {
            let mut c = FileCursor::new(path.clone());
            let _ = c.read_new();
            cursores.insert(path, c);
        }
    }
    println!("cursores posicionados em {} arquivos. aguardando prompts...\n", cursores.len());

    for evento in rx {
        let evento = match evento {
            Ok(e) => e,
            Err(e) => {
                eprintln!("erro do watcher, seguindo: {e}");
                continue;
            }
        };

        for path in evento.paths {
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some((provider, sessao)) = identificar(&path) else { continue };

            let cursor = cursores
                .entry(path.clone())
                .or_insert_with(|| FileCursor::new(path.clone()));

            let linhas = match cursor.read_new() {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("erro lendo {}, seguindo: {e}", path.display());
                    continue;
                }
            };

            for linha in linhas {
                let capturado = match provider {
                    Provider::ClaudeCode => claude.parse_line(&linha, &sessao),
                    Provider::Codex => codex.parse_line(&linha, &sessao),
                };
                if let Some(e) = capturado {
                    total += 1;
                    let preview: String = e.text.chars().take(90).collect();
                    println!("[{:>4}] {:<12} {}  {}", total, e.provider.as_str(), e.timestamp, preview);
                }
            }
        }
    }

    Ok(())
}

fn jsonls_em(raiz: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    let Ok(entradas) = std::fs::read_dir(raiz) else { return saida };
    for entrada in entradas.flatten() {
        let p = entrada.path();
        if p.is_dir() {
            saida.extend(jsonls_em(&p));
        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            saida.push(p);
        }
    }
    saida
}
```

> **Correções aplicadas durante a execução.** O código acima tem três defeitos
> encontrados na revisão. A implementação em [cli/src/main.rs](../../../cli/src/main.rs)
> é a fonte de verdade desta task; o bloco acima fica como registro do ponto de
> partida.
>
> **1. Duplicação quando o arquivo é movido.** Cursores indexados por `PathBuf`.
> O Codex **move** arquivos de `~/.codex/sessions/` para
> `~/.codex/archived_sessions/` — confirmado empiricamente: 206 arquivos numa
> raiz, 1 na outra, e o UUID arquivado ausente da primeira. O evento no caminho
> novo criava cursor do zero e reimprimia a sessão inteira, falhando o critério
> "zero duplicado" deste próprio milestone. Correção: chave do mapa passa a ser
> `(Provider, session_id)`, `FileCursor` ganha `set_path` preservando `offset`,
> `buffer` e `prefixo`, e `Provider` deriva `Hash`. Funciona porque
> `sessao_do_caminho` deriva o id do nome do arquivo, não do diretório.
>
> **2. Raiz ausente no arranque nunca era observada.** `watcher.watch()` só era
> chamado para raízes existentes, sem nova tentativa — uma pasta criada depois
> ficava invisível para sempre, sem erro. Correção: raízes faltantes ficam numa
> lista e são retentadas; ao aparecer, seus cursores são posicionados no fim
> para não despejar histórico.
>
> **3. Sem rede de segurança contra evento perdido.** O laço dependia
> inteiramente do canal do `notify`, e `ReadDirectoryChangesW` pode coalescer ou
> perder eventos sob rajada de escrita — prompt sumiria sem log e sem crash.
> Correção: laço com `recv_timeout`; a cada ociosidade, revarredura das raízes
> observadas pelo **mesmo** caminho de código dos eventos. Cursores compartilhados
> tornam a revarredura idempotente.

- [ ] **Step 3: Compilar**

```bash
cargo build
```

Esperado: build sem erros.

- [ ] **Step 4: Rodar e validar manualmente**

```bash
cargo run --bin promptchi-watch
```

Com o binário rodando, abrir o Claude Code em outro terminal e enviar 3 prompts. Depois abrir o Codex e enviar 2.

Verificar:
1. Os 5 prompts aparecem, na ordem, com o provider correto
2. Nenhum `tool_result` ou resposta de agente aparece
3. Nenhum prompt aparece duas vezes
4. O contador bate exatamente com o que foi digitado

- [ ] **Step 5: Gate de M1 — dia inteiro**

Deixar o binário rodando durante um dia de trabalho real, com a saída redirecionada:

```bash
cargo run --release --bin promptchi-watch > captura.log 2>&1
```

Ao fim do dia, comparar a contagem do log com a percepção real de uso. **Gate: zero prompt perdido, zero duplicado, zero crash.**

Registrar o resultado em `docs/superpowers/notes/m1-gate.md`, incluindo qualquer divergência encontrada.

- [ ] **Step 6: Commit**

```bash
git add cli Cargo.toml docs/superpowers/notes/m1-gate.md
git commit -m "feat: binário de debug com watcher de transcripts"
```

---

## Fora deste plano

Scoring, progressão, SQLite, overlay integrado ao core, dashboard, ritual diário, backfill, empacotamento. Cada um vira um plano próprio depois que os gates de M0 e M1 passarem.

O plano seguinte (M2 — Scoring determinístico) só deve ser escrito após o gate de M1, porque a distribuição real dos prompts capturados informa a calibração da heurística.
