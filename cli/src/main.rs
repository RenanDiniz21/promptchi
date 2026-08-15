use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use promptchi_core::adapters::claude_code::ClaudeCodeAdapter;
use promptchi_core::adapters::codex::{sessao_do_caminho, CodexAdapter};
use promptchi_core::adapters::PromptSource;
use promptchi_core::cursor::FileCursor;
use promptchi_core::types::Provider;

/// Intervalo de ociosidade do laço principal. A cada estouro, o binário
/// tenta observar raízes que ainda faltavam e revarre as raízes já
/// observadas — rede de segurança contra evento perdido/coalescido pelo
/// backend nativo do watcher, e contra pastas que só passam a existir
/// depois do arranque.
const TIMEOUT_OCIOSO: Duration = Duration::from_secs(5);

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

/// Estado do processamento: cursores indexados por identidade de sessão
/// (provider + id de sessão), não por caminho de arquivo. O Codex move
/// arquivos de `sessions/` para `archived_sessions/` preservando o
/// conteúdo; indexar por caminho faria o cursor do caminho novo nascer do
/// zero e reemitir tudo o que já havia sido capturado sob o caminho antigo.
struct Estado {
    cursores: HashMap<(Provider, String), FileCursor>,
    total: usize,
}

impl Estado {
    fn new() -> Self {
        Self { cursores: HashMap::new(), total: 0 }
    }

    /// Processa um único caminho: identifica provider/sessão, garante o
    /// cursor certo (corrigindo o caminho se o arquivo foi movido), lê
    /// linhas novas e imprime os prompts humanos capturados.
    ///
    /// Compartilhada entre o laço de eventos do watcher e a revarredura
    /// periódica — as duas vias processam arquivo exatamente do mesmo jeito,
    /// para não divergir.
    fn processar(&mut self, path: &Path, claude: &ClaudeCodeAdapter, codex: &CodexAdapter) {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            return;
        }
        let Some((provider, sessao)) = identificar(path) else { return };

        let cursor = self
            .cursores
            .entry((provider, sessao.clone()))
            .or_insert_with(|| FileCursor::new(path.to_path_buf()));
        // Idempotente quando o caminho não mudou; corrige o cursor quando
        // o arquivo foi movido para outra raiz preservando offset/buffer.
        cursor.set_path(path.to_path_buf());

        let linhas = match cursor.read_new() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("erro lendo {}, seguindo: {e}", path.display());
                return;
            }
        };

        for linha in linhas {
            let capturado = match provider {
                Provider::ClaudeCode => claude.parse_line(&linha, &sessao),
                Provider::Codex => codex.parse_line(&linha, &sessao),
            };
            if let Some(e) = capturado {
                self.total += 1;
                let preview: String = e.text.chars().take(90).collect();
                println!("[{:>4}] {:<12} {}  {}", self.total, e.provider.as_str(), e.timestamp, preview);
            }
        }
    }

    /// Posiciona os cursores dos arquivos existentes de uma raiz no FIM,
    /// sem imprimir nada. Usado no arranque e quando uma raiz que faltava
    /// passa a existir — para nunca despejar histórico.
    fn posicionar_no_fim(&mut self, raiz: &Path) {
        for path in jsonls_em(raiz) {
            let Some((provider, sessao)) = identificar(&path) else { continue };
            let cursor = self
                .cursores
                .entry((provider, sessao))
                .or_insert_with(|| FileCursor::new(path.clone()));
            cursor.set_path(path.clone());
            let _ = cursor.read_new();
        }
    }
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

    let mut estado = Estado::new();
    let mut observadas: Vec<PathBuf> = Vec::new();
    let mut faltando: Vec<PathBuf> = Vec::new();

    for raiz in &raizes {
        if raiz.exists() {
            watcher.watch(raiz, RecursiveMode::Recursive)?;
            println!("observando: {}", raiz.display());
            // Só prompts novos: posiciona os cursores no fim dos arquivos existentes.
            estado.posicionar_no_fim(raiz);
            observadas.push(raiz.clone());
        } else {
            eprintln!("aviso: raiz ausente, tentando de novo periodicamente: {}", raiz.display());
            faltando.push(raiz.clone());
        }
    }
    println!("cursores posicionados em {} arquivos. aguardando prompts...\n", estado.cursores.len());

    let claude = ClaudeCodeAdapter;
    let codex = CodexAdapter;

    loop {
        match rx.recv_timeout(TIMEOUT_OCIOSO) {
            Ok(Ok(evento)) => {
                for path in evento.paths {
                    estado.processar(&path, &claude, &codex);
                }
            }
            Ok(Err(e)) => {
                eprintln!("erro do watcher, seguindo: {e}");
            }
            Err(RecvTimeoutError::Timeout) => {
                // Ocioso: (a) tenta observar raízes que ainda faltavam.
                faltando.retain(|raiz| {
                    if !raiz.exists() {
                        return true; // continua faltando
                    }
                    match watcher.watch(raiz, RecursiveMode::Recursive) {
                        Ok(()) => {
                            println!("raiz apareceu, agora observando: {}", raiz.display());
                            estado.posicionar_no_fim(raiz);
                            observadas.push(raiz.clone());
                            false // sai da lista de faltando
                        }
                        Err(e) => {
                            eprintln!("erro observando {}, tentando de novo depois: {e}", raiz.display());
                            true
                        }
                    }
                });

                // (b) revarredura das raízes já observadas: rede de
                // segurança contra evento perdido/coalescido pelo backend
                // nativo. Cursores existentes só entregam bytes novos, então
                // revarrer é barato e idempotente; arquivo novo que o
                // watcher não notificou é descoberto aqui.
                for raiz in &observadas {
                    for path in jsonls_em(raiz) {
                        estado.processar(&path, &claude, &codex);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
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
