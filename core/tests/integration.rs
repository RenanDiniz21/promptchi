use std::path::PathBuf;

use promptchi_core::adapters::claude_code::ClaudeCodeAdapter;
use promptchi_core::adapters::codex::{sessao_do_caminho, CodexAdapter};
use promptchi_core::adapters::PromptSource;
use promptchi_core::cursor::FileCursor;
use promptchi_core::types::{PromptEvent, Provider};

fn fixture(nome: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(nome)
}

/// Caminho completo de captura do Codex: cursor entrega linhas, cada linha
/// passa por `registrar_sessao` (para o `session_meta` ser reconhecido) e
/// depois por `parse_line`. É exatamente a sequência que o binário usa.
fn capturar_codex(cursor: &mut FileCursor, adapter: &mut CodexAdapter, sessao: &str) -> Vec<PromptEvent> {
    let linhas = cursor.read_new().unwrap();
    let mut eventos = Vec::new();
    for l in &linhas {
        adapter.registrar_sessao(l, sessao);
        if let Some(e) = adapter.parse_line(l, sessao) {
            eventos.push(e);
        }
    }
    eventos
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
    let mut adapter = CodexAdapter::new();

    let eventos = capturar_codex(&mut cursor, &mut adapter, "sessao-teste");

    assert_eq!(eventos.len(), 2, "esperado 2 prompts humanos de 4 linhas");
    assert_eq!(eventos[0].text, "o worker de refugo esta ignorando os ciclos");
    assert_eq!(eventos[1].text, "so dispara se todos forem refugo");
}

#[test]
fn replay_de_sessao_de_subagente_do_codex_nao_captura_nada() {
    let mut cursor = FileCursor::new(fixture("codex_subagente.jsonl"));
    let mut adapter = CodexAdapter::new();

    let eventos = capturar_codex(&mut cursor, &mut adapter, "sessao-subagente");

    assert!(eventos.is_empty(), "sessão de subagente não pode emitir prompt: {eventos:?}");
}

#[test]
fn segunda_leitura_nao_reprocessa_nada() {
    let mut cursor = FileCursor::new(fixture("claude_code.jsonl"));
    assert_eq!(cursor.read_new().unwrap().len(), 6);
    assert_eq!(cursor.read_new().unwrap().len(), 0, "duplicação de prompts");
}

/// Ponta a ponta: cursor + adapter sobre um arquivo que cresce em pedaços,
/// inclusive com linha partida no meio. Nenhum prompt pode ser emitido duas
/// vezes nem sumir.
#[test]
fn crescimento_incremental_nao_duplica_nem_perde_prompt() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("sessao.jsonl");

    let acrescentar = |texto: &str| {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).unwrap();
        f.write_all(texto.as_bytes()).unwrap();
        f.flush().unwrap();
    };

    let mut cursor = FileCursor::new(p.clone());
    let adapter = ClaudeCodeAdapter;
    let mut capturados: Vec<String> = Vec::new();

    let ler = |cursor: &mut FileCursor, capturados: &mut Vec<String>| {
        for l in cursor.read_new().unwrap() {
            if let Some(e) = adapter.parse_line(&l, "s1") {
                capturados.push(e.text);
            }
        }
    };

    // Nada ainda.
    acrescentar(r#"{"type":"user","message":{"role":"user","content":"primeiro prompt"},"timestamp":"t1"}"#);
    ler(&mut cursor, &mut capturados);
    assert!(capturados.is_empty(), "linha sem \\n não pode ser emitida");

    // Fecha a primeira linha e abre a segunda pela metade.
    acrescentar("\n{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"segundo ");
    ler(&mut cursor, &mut capturados);
    assert_eq!(capturados, vec!["primeiro prompt".to_string()]);

    // Releitura sem escrita nova não pode emitir nada.
    ler(&mut cursor, &mut capturados);
    assert_eq!(capturados, vec!["primeiro prompt".to_string()], "duplicou sem escrita nova");

    acrescentar("prompt\"},\"timestamp\":\"t2\"}\n");
    ler(&mut cursor, &mut capturados);
    assert_eq!(
        capturados,
        vec!["primeiro prompt".to_string(), "segundo prompt".to_string()]
    );

    // Três releituras seguidas: idempotência é a propriedade do gate.
    for _ in 0..3 {
        ler(&mut cursor, &mut capturados);
    }
    assert_eq!(capturados.len(), 2, "revarredura duplicou prompt");
}

#[test]
fn sessao_do_codex_vem_do_nome_do_arquivo() {
    assert_eq!(
        sessao_do_caminho("x/rollout-2026-06-24T12-52-17-019efa54-f763-7691-91d3-c9a38153c864.jsonl").as_deref(),
        Some("019efa54-f763-7691-91d3-c9a38153c864")
    );
}
