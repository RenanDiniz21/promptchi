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

use tauri::{LogicalPosition, LogicalSize, Manager, Position, Size};

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

#[tauri::command]
fn set_mode(window: tauri::WebviewWindow, mode: String) -> Result<(), String> {
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
fn set_click_through(window: tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let w = app
                .get_webview_window(WINDOW_LABEL)
                .expect("janela 'pet' não encontrada — ver label em tauri.conf.json");
            // Click-through começa DESLIGADO: se começasse ligado, a janela
            // nunca receberia clique/foco para o dono trocar de modo pelo
            // teclado (ver .superpowers task-2-brief + desvio no prompt).
            w.set_ignore_cursor_events(false)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_mode,
            set_click_through,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar spike");
}
