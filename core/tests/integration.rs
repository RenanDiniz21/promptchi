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
