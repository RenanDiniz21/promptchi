// Spike M0 — descartável. Sem otimização, sem abstração.
//
// Alterna os dois modos de verificação em runtime, sem editar arquivo:
//   G      -> modo grade (folha inteira + grade sobreposta)
//   A      -> modo overlay (sprite animado, janela transparente pequena)
//   C      -> liga/desliga click-through
//   Escape -> fecha o app
//
// Começa em modo overlay com click-through DESLIGADO (senão a janela não
// recebe teclado — ver nota no prompt/brief).
//
// Click-through NÃO fica ligado pra sempre: o lado Rust (lib.rs) desliga
// sozinho ao perder foco (o que acontece naturalmente ao testar, já que
// clicar "através" da janela dá foco ao app de baixo) e tem um timer de
// segurança (CLICK_THROUGH_AUTO_OFF_SECS em lib.rs) como rede. Este
// arquivo só reflete o estado real vindo do Rust via polling — não
// assume que o clique em C é a única forma de desligar.

const { invoke } = window.__TAURI__.core;

let mode = "overlay";
let clickThrough = false;

function applyMode(nextMode) {
  mode = nextMode;
  document.body.classList.remove("mode-grid", "mode-overlay");
  document.body.classList.add(mode === "grid" ? "mode-grid" : "mode-overlay");
  invoke("set_mode", { mode }).catch((err) =>
    console.error("set_mode falhou:", err)
  );
}

function requestClickThrough(enabled) {
  invoke("set_click_through", { enabled }).catch((err) =>
    console.error("set_click_through falhou:", err)
  );
}

function renderClickThroughStatus(status) {
  clickThrough = status.enabled;
  const el = document.getElementById("click-through-state");
  if (!el) return;
  el.textContent = status.enabled
    ? `ON — desliga sozinho em ${status.seconds_left}s (ou ao perder foco)`
    : "OFF";
}

function pollClickThroughStatus() {
  invoke("get_click_through_state")
    .then(renderClickThroughStatus)
    .catch((err) => console.error("get_click_through_state falhou:", err));
}

function fillGridInfo() {
  const cs = getComputedStyle(document.documentElement);
  const get = (name) => cs.getPropertyValue(name).trim();
  const set = (id, value) => {
    const el = document.getElementById(id);
    if (el) el.textContent = value;
  };
  set("info-cell", `${get("--cell-w")} x ${get("--cell-h")}`);
  set("info-cols", get("--cols"));
  set("info-rows", get("--rows"));
  set("info-sheet", `${get("--sheet-w")} x ${get("--sheet-h")}`);
}

window.addEventListener("DOMContentLoaded", () => {
  fillGridInfo();
  applyMode("overlay");
  requestClickThrough(false);
  pollClickThroughStatus();
  // Roda o tempo todo (barato, 2x/s). Não vaza pixel no modo overlay
  // porque #help continua "display: none" lá (ver styles.css) — só
  // atualiza um texto que fica invisível.
  setInterval(pollClickThroughStatus, 500);

  window.addEventListener("keydown", (e) => {
    switch (e.key) {
      case "g":
      case "G":
        applyMode("grid");
        break;
      case "a":
      case "A":
        applyMode("overlay");
        break;
      case "c":
      case "C":
        requestClickThrough(!clickThrough);
        break;
      case "Escape":
        invoke("quit_app").catch((err) =>
          console.error("quit_app falhou:", err)
        );
        break;
      default:
        break;
    }
  });
});
