// Spike M0 — descartável. Sem otimização, sem abstração.
//
// Responde UMA pergunta: overlay transparente + always-on-top + sprite
// animado + click-through funcionam juntos nesta máquina?
//
// Dois modos alternáveis em runtime (tecla G / A no frontend), sem editar
// arquivo nenhum:
//   - "grid":    janela grande, aparência opaca (simulada via CSS — ver
//                nota abaixo), mostra a folha inteira com grade sobreposta.
//   - "overlay": janela pequena, transparente de verdade, só o sprite.
//
// NOTA IMPORTANTE sobre transparência:
// A propriedade `transparent` do Tauri é fixada na CRIAÇÃO da janela
// (tauri.conf.json) e não tem setter em runtime na v2. Por isso a janela é
// criada `transparent: true` sempre — é o único jeito de testar a
// transparência real de verdade (o objetivo do gate). O modo "grid" não
// torna a janela opaca de fato: em vez disso, o CSS pinta o fundo inteiro
// com uma cor sólida, cobrindo 100% dos pixels da janela, o que produz o
// mesmo resultado visual de uma janela opaca. Ver styles.css.
//
// O que É alternável em runtime pela API do Tauri e É usado aqui:
// tamanho/posição da janela (set_size/set_position) e click-through
// (set_ignore_cursor_events). Ambos mudam via comandos abaixo.
//
// NOTA sobre click-through sem beco sem saída:
// Com click-through ligado e a janela fora da barra de tarefas/Alt+Tab,
// clicar "através" dela pra testar o gate tira o foco da janela do spike
// (o clique chega no app de baixo, que ganha foco) — depois disso teclado
// nenhum chega mais nela pra desligar o click-through de volta. Por isso
// o desligamento NÃO depende só da tecla C: ver `on_window_event` no fim
// do arquivo (desliga ao perder foco — o próprio ato de testar já
// devolve o controle) e o timer em `CLICK_THROUGH_AUTO_OFF_SECS` (rede
// de segurança caso o evento de foco não sirva nesta plataforma).

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{LogicalPosition, LogicalSize, Manager, Position, Size, WebviewWindow, WindowEvent};

// `on_window_event` entrega `&tauri::Window`, mas os comandos (injetados
// pelo Tauri a partir do invoke) recebem `tauri::WebviewWindow` — os dois
// tipos têm `set_ignore_cursor_events` mas não compartilham um trait
// nem um Deref entre si. Este trait local deixa uma única
// `disable_click_through_locked` valer pros dois, sem duplicar a lógica.
trait CursorEvents {
    fn set_ignore(&self, ignore: bool) -> tauri::Result<()>;
}
impl<R: tauri::Runtime> CursorEvents for tauri::Window<R> {
    fn set_ignore(&self, ignore: bool) -> tauri::Result<()> {
        self.set_ignore_cursor_events(ignore)
    }
}
impl<R: tauri::Runtime> CursorEvents for WebviewWindow<R> {
    fn set_ignore(&self, ignore: bool) -> tauri::Result<()> {
        self.set_ignore_cursor_events(ignore)
    }
}

const WINDOW_LABEL: &str = "pet";

// ===== Geometria da janela por modo — ajustar aqui se necessário =====
const OVERLAY_WIDTH: f64 = 240.0;
const OVERLAY_HEIGHT: f64 = 240.0;
const OVERLAY_X: f64 = 1500.0;
const OVERLAY_Y: f64 = 800.0;

const GRID_WIDTH: f64 = 1000.0;
const GRID_HEIGHT: f64 = 900.0;
const GRID_X: f64 = 80.0;
const GRID_Y: f64 = 40.0;
// =======================================================================

// ===== Click-through com auto-desligamento — ajustar aqui =====
// Quanto tempo (em segundos) o click-through fica ligado no máximo antes
// de desligar sozinho, mesmo se nada mais acontecer. Aumentar se 10s for
// pouco pra posicionar a janela e clicar através dela.
const CLICK_THROUGH_AUTO_OFF_SECS: u64 = 10;
// =======================================================================

#[derive(Default)]
struct ClickThroughState {
    enabled: bool,
    // Incrementado a cada mudança de estado (manual, por perda de foco ou
    // pelo timer). Um timer agendado só age se essa geração não mudou
    // desde que ele foi agendado — assim, ligar/desligar de novo ou
    // perder foco durante a contagem cancela o timer antigo sem precisar
    // de nenhum mecanismo de cancelamento explícito.
    generation: u64,
    deadline: Option<Instant>,
}

type ClickThroughStateHandle = Mutex<ClickThroughState>;

#[derive(serde::Serialize)]
struct ClickThroughStatus {
    enabled: bool,
    seconds_left: u64,
}

/// Desliga o click-through. Chamado pela tecla C, pela perda de foco da
/// janela e pelo timer de segurança — sempre pelo mesmo caminho, pra não
/// ter três lugares reimplementando a mesma lógica. Genérico sobre
/// `CursorEvents` porque quem chama tem ora um `Window` (evento de
/// foco), ora um `WebviewWindow` (comandos) — ver trait acima.
fn disable_click_through_locked<W: CursorEvents>(window: &W, guard: &mut ClickThroughState) {
    guard.enabled = false;
    guard.deadline = None;
    guard.generation += 1;
    let _ = window.set_ignore(false);
}

#[tauri::command]
fn set_mode(window: WebviewWindow, mode: String) -> Result<(), String> {
    let (w, h, x, y) = match mode.as_str() {
        "grid" => (GRID_WIDTH, GRID_HEIGHT, GRID_X, GRID_Y),
        "overlay" => (OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_X, OVERLAY_Y),
        other => return Err(format!("modo desconhecido: {other}")),
    };
    window
        .set_size(Size::Logical(LogicalSize::new(w, h)))
        .map_err(|e| e.to_string())?;
    window
        .set_position(Position::Logical(LogicalPosition::new(x, y)))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_click_through(
    window: WebviewWindow,
    state: tauri::State<ClickThroughStateHandle>,
    enabled: bool,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;

    if !enabled {
        disable_click_through_locked(&window, &mut guard);
        return Ok(());
    }

    guard.generation += 1;
    let my_generation = guard.generation;
    guard.enabled = true;
    guard.deadline = Some(Instant::now() + Duration::from_secs(CLICK_THROUGH_AUTO_OFF_SECS));
    window
        .set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    drop(guard);

    // Timer de segurança: se nada desligar o click-through antes (nem a
    // tecla C, nem a perda de foco), esta thread desliga sozinha. Uma
    // std::thread simples (sem tokio) porque o processo inteiro morre
    // junto com o app — não fica nada "pendurado" ao fechar.
    let window_for_timer = window.clone();
    let app_handle = window.app_handle().clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(CLICK_THROUGH_AUTO_OFF_SECS));
        let state = app_handle.state::<ClickThroughStateHandle>();
        let mut guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return, // app fechando: nada a fazer
        };
        // Só age se nada mudou o estado desde que este timer foi
        // agendado (nem outro C, nem perda de foco já desligaram antes).
        if guard.generation == my_generation && guard.enabled {
            disable_click_through_locked(&window_for_timer, &mut guard);
        }
    });

    Ok(())
}

#[tauri::command]
fn get_click_through_state(state: tauri::State<ClickThroughStateHandle>) -> ClickThroughStatus {
    let guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let seconds_left = guard
        .deadline
        .map(|d| {
            d.saturating_duration_since(Instant::now())
                .as_secs_f64()
                .ceil() as u64
        })
        .unwrap_or(0);
    ClickThroughStatus {
        enabled: guard.enabled,
        seconds_left,
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ClickThroughStateHandle::default())
        .setup(|app| {
            let w = app
                .get_webview_window(WINDOW_LABEL)
                .expect("janela 'pet' não encontrada — ver label em tauri.conf.json");
            // Click-through começa DESLIGADO: se começasse ligado, a janela
            // nunca receberia clique/foco para o dono trocar de modo pelo
            // teclado.
            w.set_ignore_cursor_events(false)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != WINDOW_LABEL {
                return;
            }
            // Mecanismo PRIMÁRIO de recuperação: testar o click-through
            // (clicar "através" da janela em outro app) naturalmente tira
            // o foco desta janela — é exatamente esse momento que usamos
            // pra desligar sozinho, antes mesmo do timer de segurança
            // entrar em ação.
            if let WindowEvent::Focused(false) = event {
                let state = window.app_handle().state::<ClickThroughStateHandle>();
                let mut guard = match state.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if guard.enabled {
                    disable_click_through_locked(window, &mut guard);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            set_mode,
            set_click_through,
            get_click_through_state,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar spike");
}
