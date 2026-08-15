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
