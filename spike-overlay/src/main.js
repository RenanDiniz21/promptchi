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

function applyClickThrough(enabled) {
  clickThrough = enabled;
  invoke("set_click_through", { enabled }).catch((err) =>
    console.error("set_click_through falhou:", err)
  );
  const el = document.getElementById("click-through-state");
  if (el) el.textContent = enabled ? "ON" : "OFF";
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
  applyClickThrough(false);

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
        applyClickThrough(!clickThrough);
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
