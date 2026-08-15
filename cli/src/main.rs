mod dedup;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use promptchi_core::adapters::claude_code::ClaudeCodeAdapter;
use promptchi_core::adapters::codex::{sessao_do_caminho, CodexAdapter};
use promptchi_core::adapters::PromptSource;
use promptchi_core::cursor::FileCursor;
use promptchi_core::types::Provider;

use dedup::{ConjuntoDeDuplicados, CAPACIDADE_DEDUP};

/// Intervalo de ociosidade do laço principal. Curto de propósito: é o que
/// mantém a resposta a evento do watcher rápida. Não é o intervalo da
/// revarredura.
const TIMEOUT_OCIOSO: Duration = Duration::from_secs(5);

/// Intervalo da revarredura completa das raízes observadas.
///
/// A revarredura é REDE DE SEGURANÇA contra evento perdido ou coalescido
/// pelo backend nativo do watcher, não o mecanismo principal de captura. A
/// cada 5 segundos ela custava, com os 443 arquivos de hoje, cerca de 7,6
/// milhões de aberturas de arquivo por dia — e `~/.codex/sessions` ganha uma pasta
/// datada por dia, para sempre. A 3 minutos, e pulando por `mtime` o que não
/// mudou, o custo fica desprezível sem perder a rede de segurança.
const INTERVALO_REVARREDURA: Duration = Duration::from_secs(180);

/// Intervalo do resumo de diagnóstico no stderr.
const INTERVALO_RESUMO: Duration = Duration::from_secs(300);

/// Teto de profundidade da recursão que varre diretórios. É a única recursão
/// de um processo que não pode morrer, e ela roda periodicamente: um link
/// simbólico apontando para um ancestral faria a pilha estourar. As raízes
/// reais são rasas (`~/.codex/sessions/2026/08/14/` é profundidade 3).
const PROFUNDIDADE_MAXIMA: usize = 16;

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

/// Contagem por provider, para o dono conseguir distinguir "capturei 200
/// prompts" de "li 40 mil linhas e rejeitei todas".
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Contadores {
    linhas: u64,
    eventos: u64,
    rejeitadas: u64,
    duplicadas: u64,
}

impl Contadores {
    fn menos(&self, outro: &Contadores) -> Contadores {
        Contadores {
            linhas: self.linhas.saturating_sub(outro.linhas),
            eventos: self.eventos.saturating_sub(outro.eventos),
            rejeitadas: self.rejeitadas.saturating_sub(outro.rejeitadas),
            duplicadas: self.duplicadas.saturating_sub(outro.duplicadas),
        }
    }

    fn houve_movimento(&self) -> bool {
        *self != Contadores::default()
    }
}

/// Estado do processamento: cursores indexados por identidade de sessão
/// (provider + id de sessão), não por caminho de arquivo. O Codex move
/// arquivos de `sessions/` para `archived_sessions/` preservando o
/// conteúdo; indexar por caminho faria o cursor do caminho novo nascer do
/// zero e reemitir tudo o que já havia sido capturado sob o caminho antigo.
struct Estado {
    cursores: HashMap<(Provider, String), FileCursor>,
    /// `mtime` da última vez que cada arquivo foi considerado na
    /// revarredura, para pular os que não mudaram.
    mtimes: HashMap<PathBuf, SystemTime>,
    dedup: ConjuntoDeDuplicados,
    contadores: HashMap<Provider, Contadores>,
    /// Contadores no último resumo impresso, para calcular o delta.
    contadores_do_ultimo_resumo: HashMap<Provider, Contadores>,
    /// Sessões do Codex já avisadas por não terem `session_meta` observado,
    /// para o aviso sair uma vez só.
    avisadas_sem_meta: HashSet<String>,
    total: usize,
}

impl Estado {
    fn new() -> Self {
        Self {
            cursores: HashMap::new(),
            mtimes: HashMap::new(),
            dedup: ConjuntoDeDuplicados::new(CAPACIDADE_DEDUP),
            contadores: HashMap::new(),
            contadores_do_ultimo_resumo: HashMap::new(),
            avisadas_sem_meta: HashSet::new(),
            total: 0,
        }
    }

    /// Processa um único caminho: identifica provider/sessão, garante o
    /// cursor certo (corrigindo o caminho se o arquivo foi movido), lê
    /// linhas novas e imprime os prompts humanos ainda não vistos.
    ///
    /// Compartilhada entre o laço de eventos do watcher e a revarredura
    /// periódica — as duas vias processam arquivo exatamente do mesmo jeito,
    /// para não divergir. `emitir == false` percorre as mesmas linhas sem
    /// imprimir nem contar: é o posicionamento inicial dos cursores, que
    /// precisa passar por aqui para o `session_meta` de cada sessão do Codex
    /// ser reconhecido.
    fn processar(
        &mut self,
        path: &Path,
        claude: &ClaudeCodeAdapter,
        codex: &mut CodexAdapter,
        emitir: bool,
    ) {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            return;
        }
        let Some((provider, sessao)) = identificar(path) else { return };

        self.anotar_mtime(path);

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
        if linhas.is_empty() {
            return;
        }

        for linha in &linhas {
            // O Codex só diz que a thread é de subagente no `session_meta`,
            // a primeira linha do arquivo. Toda linha passa por aqui,
            // inclusive no posicionamento, senão a origem da sessão nunca
            // seria conhecida e os prompts dela seriam descartados.
            if provider == Provider::Codex {
                codex.registrar_sessao(linha, &sessao);
            }
            if !emitir {
                continue;
            }

            self.contadores.entry(provider).or_default().linhas += 1;

            let capturado = match provider {
                Provider::ClaudeCode => claude.parse_line(linha, &sessao),
                Provider::Codex => codex.parse_line(linha, &sessao),
            };
            let Some(evento) = capturado else {
                self.contadores.entry(provider).or_default().rejeitadas += 1;
                continue;
            };

            if !self.dedup.registrar(evento.provider, &evento.text) {
                self.contadores.entry(provider).or_default().duplicadas += 1;
                continue;
            }

            self.contadores.entry(provider).or_default().eventos += 1;
            self.total += 1;
            let preview: String = evento.text.chars().take(90).collect();
            println!(
                "[{:>4}] {:<12} {}  {}",
                self.total,
                evento.provider.as_str(),
                evento.timestamp,
                preview
            );
        }

        // Sessão do Codex cujo `session_meta` nunca foi visto não emite
        // nada (escolha conservadora do adapter). Isso não pode ser
        // silencioso: seria perda total de captura para essa sessão.
        if emitir
            && provider == Provider::Codex
            && codex.origem(&sessao).is_none()
            && self.avisadas_sem_meta.insert(sessao.clone())
        {
            eprintln!(
                "aviso: sessao codex {sessao} sem session_meta observado; \
                 prompts dela sao ignorados por precaucao ({})",
                path.display()
            );
        }
    }

    /// Guarda o `mtime` corrente ANTES da leitura. Se uma escrita cair entre
    /// a consulta e a leitura, o valor guardado fica velho e a revarredura
    /// seguinte processa o arquivo de novo — errar para o lado de reler é
    /// barato (o cursor só entrega bytes novos) e não perde prompt.
    fn anotar_mtime(&mut self, path: &Path) {
        if let Ok(m) = std::fs::metadata(path).and_then(|md| md.modified()) {
            self.mtimes.insert(path.to_path_buf(), m);
        }
    }

    /// `true` se o arquivo mudou (ou se não deu para saber) desde a última
    /// vez que passou por aqui. Filtro de candidatos da revarredura; quem
    /// processa continua sendo o mesmo `processar` do caminho de evento.
    fn mudou_desde_a_ultima_vez(&self, path: &Path) -> bool {
        let Ok(atual) = std::fs::metadata(path).and_then(|md| md.modified()) else {
            return true;
        };
        self.mtimes.get(path) != Some(&atual)
    }

    /// Posiciona os cursores dos arquivos existentes de uma raiz no FIM,
    /// sem imprimir nada. Usado no arranque e quando uma raiz que faltava
    /// passa a existir — para nunca despejar histórico.
    fn posicionar_no_fim(
        &mut self,
        raiz: &Path,
        claude: &ClaudeCodeAdapter,
        codex: &mut CodexAdapter,
    ) {
        for path in jsonls_em(raiz) {
            self.processar(&path, claude, codex, false);
        }
    }

    /// Revarredura: rede de segurança contra evento perdido/coalescido pelo
    /// backend nativo e contra arquivo novo que não gerou notificação.
    fn revarrer(&mut self, raizes: &[PathBuf], claude: &ClaudeCodeAdapter, codex: &mut CodexAdapter) {
        for raiz in raizes {
            for path in jsonls_em(raiz) {
                if !self.mudou_desde_a_ultima_vez(&path) {
                    continue;
                }
                self.processar(&path, claude, codex, true);
            }
        }
    }

    /// Resumo periódico no stderr. Só sai se houve movimento desde o último:
    /// um dia calmo não vira ruído, e um dia em que a captura quebrou fica
    /// visível como "muitas linhas, zero prompts".
    fn imprimir_resumo(&mut self, desde_o_arranque: Duration) {
        let mut linhas: Vec<String> = Vec::new();
        for (provider, atual) in &self.contadores {
            let anterior = self.contadores_do_ultimo_resumo.get(provider).copied().unwrap_or_default();
            let delta = atual.menos(&anterior);
            if !delta.houve_movimento() {
                continue;
            }
            linhas.push(format!(
                "  {:<12} total: {} linhas, {} prompts, {} rejeitadas, {} duplicadas | \
                 desde o ultimo resumo: +{} linhas, +{} prompts, +{} rejeitadas, +{} duplicadas",
                provider.as_str(),
                atual.linhas,
                atual.eventos,
                atual.rejeitadas,
                atual.duplicadas,
                delta.linhas,
                delta.eventos,
                delta.rejeitadas,
                delta.duplicadas,
            ));
        }
        if linhas.is_empty() {
            return;
        }
        eprintln!(
            "resumo apos {} min de execucao ({} sessoes, {} prompts na janela de deduplicacao):",
            desde_o_arranque.as_secs() / 60,
            self.cursores.len(),
            self.dedup.len()
        );
        for l in linhas {
            eprintln!("{l}");
        }
        self.contadores_do_ultimo_resumo = self.contadores.clone();
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
    let claude = ClaudeCodeAdapter;
    let mut codex = CodexAdapter::new();
    let mut observadas: Vec<PathBuf> = Vec::new();
    let mut faltando: Vec<PathBuf> = Vec::new();

    for raiz in &raizes {
        if !raiz.exists() {
            eprintln!("aviso: raiz ausente, tentando de novo periodicamente: {}", raiz.display());
            faltando.push(raiz.clone());
            continue;
        }
        // Falha ao observar uma raiz existente não pode derrubar o
        // processo: vira o mesmo caso de "raiz faltando" e é tentada de
        // novo a cada revarredura.
        match watcher.watch(raiz, RecursiveMode::Recursive) {
            Ok(()) => {
                println!("observando: {}", raiz.display());
                // Só prompts novos: posiciona os cursores no fim dos arquivos existentes.
                estado.posicionar_no_fim(raiz, &claude, &mut codex);
                observadas.push(raiz.clone());
            }
            Err(e) => {
                eprintln!("aviso: nao consegui observar {}, tentando de novo depois: {e}", raiz.display());
                faltando.push(raiz.clone());
            }
        }
    }
    println!("cursores posicionados em {} arquivos. aguardando prompts...\n", estado.cursores.len());

    let arranque = Instant::now();
    let mut ultima_revarredura = Instant::now();
    let mut ultimo_resumo = Instant::now();

    loop {
        match rx.recv_timeout(TIMEOUT_OCIOSO) {
            Ok(Ok(evento)) => {
                for path in evento.paths {
                    estado.processar(&path, &claude, &mut codex, true);
                }
            }
            Ok(Err(e)) => {
                eprintln!("erro do watcher, seguindo: {e}");
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // Tarefas periódicas fora do braço de timeout: um dia movimentado,
        // em que eventos chegam sem parar, também precisa de revarredura e
        // de resumo.
        if ultima_revarredura.elapsed() >= INTERVALO_REVARREDURA {
            ultima_revarredura = Instant::now();

            // (a) raízes que ainda faltavam.
            faltando.retain(|raiz| {
                if !raiz.exists() {
                    return true; // continua faltando
                }
                match watcher.watch(raiz, RecursiveMode::Recursive) {
                    Ok(()) => {
                        println!("raiz apareceu, agora observando: {}", raiz.display());
                        estado.posicionar_no_fim(raiz, &claude, &mut codex);
                        observadas.push(raiz.clone());
                        false // sai da lista de faltando
                    }
                    Err(e) => {
                        eprintln!("erro observando {}, tentando de novo depois: {e}", raiz.display());
                        true
                    }
                }
            });

            // (b) revarredura das raízes já observadas.
            estado.revarrer(&observadas, &claude, &mut codex);
        }

        if ultimo_resumo.elapsed() >= INTERVALO_RESUMO {
            ultimo_resumo = Instant::now();
            estado.imprimir_resumo(arranque.elapsed());
        }
    }

    Ok(())
}

fn jsonls_em(raiz: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    coletar_jsonls(raiz, 0, &mut saida);
    saida
}

fn coletar_jsonls(dir: &Path, profundidade: usize, saida: &mut Vec<PathBuf>) {
    if profundidade > PROFUNDIDADE_MAXIMA {
        eprintln!(
            "aviso: profundidade maxima ({PROFUNDIDADE_MAXIMA}) atingida, nao descendo em {}",
            dir.display()
        );
        return;
    }
    let entradas = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            // Sem isso, uma pasta ilegivel some da revarredura em silencio.
            eprintln!("aviso: nao consegui listar {}, seguindo: {e}", dir.display());
            return;
        }
    };
    for entrada in entradas {
        let entrada = match entrada {
            Ok(e) => e,
            Err(e) => {
                eprintln!("aviso: entrada ilegivel em {}, seguindo: {e}", dir.display());
                continue;
            }
        };
        let p = entrada.path();
        if p.is_dir() {
            coletar_jsonls(&p, profundidade + 1, saida);
        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            saida.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identificar_reconhece_as_duas_raizes() {
        let claude = PathBuf::from("C:/Users/x/.claude/projects/proj/abc-123.jsonl");
        assert_eq!(
            identificar(&claude),
            Some((Provider::ClaudeCode, "abc-123".to_string()))
        );

        let codex = PathBuf::from(
            "C:/Users/x/.codex/sessions/2026/08/14/rollout-2026-08-14T12-52-17-019efa54-f763-7691-91d3-c9a38153c864.jsonl",
        );
        assert_eq!(
            identificar(&codex),
            Some((Provider::Codex, "019efa54-f763-7691-91d3-c9a38153c864".to_string()))
        );

        assert_eq!(identificar(&PathBuf::from("C:/outro/lugar/x.jsonl")), None);
    }

    #[test]
    fn varredura_encontra_jsonl_em_subpastas() {
        let dir = tempfile::tempdir().unwrap();
        let fundo = dir.path().join("2026").join("08").join("14");
        std::fs::create_dir_all(&fundo).unwrap();
        std::fs::write(fundo.join("rollout-a.jsonl"), b"{}\n").unwrap();
        std::fs::write(dir.path().join("raso.jsonl"), b"{}\n").unwrap();
        std::fs::write(dir.path().join("ignorado.txt"), b"x").unwrap();

        let mut achados = jsonls_em(dir.path());
        achados.sort();
        assert_eq!(achados.len(), 2, "esperado 2 jsonl, achei {achados:?}");
        assert!(achados.iter().all(|p| p.extension().unwrap() == "jsonl"));
    }

    #[test]
    fn varredura_para_no_teto_de_profundidade() {
        let dir = tempfile::tempdir().unwrap();
        let mut fundo = dir.path().to_path_buf();
        for i in 0..(PROFUNDIDADE_MAXIMA + 3) {
            fundo = fundo.join(format!("n{i}"));
        }
        std::fs::create_dir_all(&fundo).unwrap();
        std::fs::write(fundo.join("profundo.jsonl"), b"{}\n").unwrap();
        std::fs::write(dir.path().join("raso.jsonl"), b"{}\n").unwrap();

        // Não estoura a pilha, não trava, e o que está antes do teto é
        // encontrado.
        let achados = jsonls_em(dir.path());
        assert_eq!(achados.len(), 1);
        assert!(achados[0].ends_with("raso.jsonl"));
    }

    #[test]
    fn diretorio_inexistente_nao_causa_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(jsonls_em(&dir.path().join("nao_existe")).is_empty());
    }

    #[test]
    fn mtime_pula_arquivo_que_nao_mudou() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jsonl");
        std::fs::write(&p, b"{}\n").unwrap();

        let mut estado = Estado::new();
        assert!(estado.mudou_desde_a_ultima_vez(&p), "arquivo nunca visto conta como mudado");
        estado.anotar_mtime(&p);
        assert!(!estado.mudou_desde_a_ultima_vez(&p), "arquivo intocado deveria ser pulado");
    }

    #[test]
    fn contadores_registram_leitura_e_rejeicao() {
        let dir = tempfile::tempdir().unwrap();
        // O caminho precisa parecer uma raiz do Claude Code para
        // `identificar` reconhecer o provider.
        let raiz = dir.path().join(".claude").join("projects").join("proj");
        std::fs::create_dir_all(&raiz).unwrap();
        let p = raiz.join("sessao-1.jsonl");
        std::fs::write(
            &p,
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"prompt de verdade"},"timestamp":"t1"}"#,
                "\n",
                r#"{"type":"assistant","message":{"role":"assistant","content":"resposta"},"timestamp":"t2"}"#,
                "\n",
                r#"{"type":"user","message":{"role":"user","content":"<system-reminder>ruido</system-reminder>"},"timestamp":"t3"}"#,
                "\n",
            )
            .as_bytes(),
        )
        .unwrap();

        let claude = ClaudeCodeAdapter;
        let mut codex = CodexAdapter::new();
        let mut estado = Estado::new();
        estado.processar(&p, &claude, &mut codex, true);

        let c = estado.contadores.get(&Provider::ClaudeCode).copied().unwrap_or_default();
        assert_eq!(c.linhas, 3);
        assert_eq!(c.eventos, 1);
        assert_eq!(c.rejeitadas, 2);
        assert_eq!(c.duplicadas, 0);
    }

    #[test]
    fn revarredura_nao_reemite_prompt_ja_capturado() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join(".claude").join("projects").join("proj");
        std::fs::create_dir_all(&raiz).unwrap();
        let p = raiz.join("sessao-1.jsonl");
        std::fs::write(
            &p,
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"prompt unico"},"timestamp":"t1"}"#,
                "\n"
            )
            .as_bytes(),
        )
        .unwrap();

        let claude = ClaudeCodeAdapter;
        let mut codex = CodexAdapter::new();
        let mut estado = Estado::new();

        estado.processar(&p, &claude, &mut codex, true);
        assert_eq!(estado.total, 1);

        // Mesmo arquivo processado de novo (evento + revarredura pisando um
        // no outro) e o mesmo texto reaparecendo numa sessão nova, que é o
        // que o fork do Codex faz.
        estado.processar(&p, &claude, &mut codex, true);
        let outra = raiz.join("sessao-2.jsonl");
        std::fs::copy(&p, &outra).unwrap();
        estado.processar(&outra, &claude, &mut codex, true);

        assert_eq!(estado.total, 1, "prompt duplicado foi emitido");
        assert_eq!(
            estado.contadores.get(&Provider::ClaudeCode).copied().unwrap_or_default().duplicadas,
            1
        );
    }

    #[test]
    fn posicionamento_nao_emite_mas_reconhece_sessao_do_codex() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join(".codex").join("sessions");
        std::fs::create_dir_all(&raiz).unwrap();
        let p = raiz.join("rollout-2026-08-14T12-52-17-019efa54-f763-7691-91d3-c9a38153c864.jsonl");
        std::fs::write(
            &p,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"x","source":"vscode","thread_source":"user"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"prompt antigo"},"timestamp":"t1"}"#,
                "\n",
            )
            .as_bytes(),
        )
        .unwrap();

        let claude = ClaudeCodeAdapter;
        let mut codex = CodexAdapter::new();
        let mut estado = Estado::new();

        estado.posicionar_no_fim(&raiz, &claude, &mut codex);
        assert_eq!(estado.total, 0, "posicionamento nao pode despejar historico");
        assert!(
            codex.origem("019efa54-f763-7691-91d3-c9a38153c864").is_some(),
            "o session_meta precisa ser reconhecido durante o posicionamento"
        );

        // Prompt novo, depois do posicionamento, é capturado normalmente.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(
            concat!(
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"prompt novo"},"timestamp":"t2"}"#,
                "\n"
            )
            .as_bytes(),
        )
        .unwrap();
        drop(f);

        estado.processar(&p, &claude, &mut codex, true);
        assert_eq!(estado.total, 1);
    }

    #[test]
    fn sessao_de_subagente_do_codex_nao_emite_pelo_caminho_do_binario() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join(".codex").join("sessions");
        std::fs::create_dir_all(&raiz).unwrap();
        let p = raiz.join("rollout-2026-08-14T12-52-17-019eac4f-2d6a-71f1-947e-84f267e9d3b4.jsonl");
        std::fs::write(
            &p,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"x","parent_thread_id":"p","source":{"subagent":{"other":"guardian"}},"thread_source":"subagent"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"The following is the Codex agent history"},"timestamp":"t1"}"#,
                "\n",
            )
            .as_bytes(),
        )
        .unwrap();

        let claude = ClaudeCodeAdapter;
        let mut codex = CodexAdapter::new();
        let mut estado = Estado::new();
        estado.processar(&p, &claude, &mut codex, true);

        assert_eq!(estado.total, 0, "prompt de subagente vazou");
        let c = estado.contadores.get(&Provider::Codex).copied().unwrap_or_default();
        assert_eq!(c.linhas, 2);
        assert_eq!(c.rejeitadas, 2);
    }
}
