# M0 — Roteiro de verificação do spike de overlay

> ## ✅ GATE APROVADO — 2026-08-15
>
> Verificado na máquina de desenvolvimento (Windows 11). Transparência,
> always-on-top, animação de sprite e click-through funcionando em conjunto.
> Grade do spritesheet confirmada sem ajuste — ver
> [`spritesheet-grid.md`](spritesheet-grid.md).
>
> **Consequência:** o design de desktop companion do spec está validado. O
> Plano B (janela opaca com cantos arredondados) não é necessário, e o
> mascote pode de fato flutuar sobre o desktop como o produto promete.
>
> O roteiro abaixo fica como registro do que foi verificado, e serve para
> repetir o teste em outra máquina ou outra GPU antes de distribuir.

**Gate eliminatório.** Se a transparência falhar nesta máquina, o design do
desktop companion muda antes de qualquer UI ser escrita.

O spike é código descartável. Sua única função é responder: *uma janela Tauri
transparente, sempre no topo, com sprite animado e cliques atravessando,
funciona aqui?*

---

## Como rodar

Em máquina nova, instalar as dependências primeiro — `node_modules/` não é
versionado:

```bash
cd spike-overlay
npm install
```

Depois:

```bash
npm run tauri dev
```

A primeira execução compila as dependências Rust do Tauri e demora alguns
minutos.

> **Telas menores que 1920x1080.** A geometria das janelas é fixa em
> `spike-overlay/src-tauri/src/lib.rs`: overlay em `1500,800`, grade de
> `1000x900` em `80,40`. Num notebook de 1366x768 alguma das duas cai
> parcialmente fora da tela. As constantes estão nomeadas e comentadas no
> topo do arquivo — ajustar antes de rodar.

Abre a janela `pet` em **modo overlay**, com click-through **desligado**.
Todo o resto é feito pelo teclado, com a janela em foco. Nenhuma edição de
arquivo é necessária durante a verificação.

| Tecla | Ação |
|---|---|
| `G` | modo grade — folha inteira com linhas de conferência |
| `A` | volta ao modo overlay |
| `C` | liga click-through (desliga sozinho) |
| `Esc` | fecha |

---

## Passo 1 — Transparência (modo overlay, já ao abrir)

Janela de 240x240 com o kuriboh animando em loop.

**Aprovado se:** só o sprite aparece. O desktop — papel de parede, outras
janelas — aparece ao redor dele. Nenhum retângulo branco, cinza ou preto.
Sem costura visível entre o último frame do loop e o primeiro.

**Reprovado se:** qualquer fundo sólido ao redor do sprite.

Conferir também: a janela **não** aparece na barra de tarefas, **não** aparece
no Alt+Tab, e fica **por cima** do VS Code e do navegador.

> Este passo é o gate. Os demais são detalhes de implementação; este é a
> pergunta que o milestone existe para responder.

## Passo 2 — Grade do spritesheet (tecla `G`)

A janela cresce para 1000x900 e mostra a folha inteira com linhas vermelhas
(verticais, a cada 192px) e azuis (horizontais, a cada 208px).

**Aprovado se:** as linhas caem nas bordas dos frames, sem cortar cabeça ou
asa e sem sobrar pixel de uma célula para outra.

**Se não encaixar:** ajustar `--cell-w` e `--cell-h` em
[`spike-overlay/src/styles.css`](../../../spike-overlay/src/styles.css). O
Tauri recarrega sozinho em dev. Depois registrar os valores finais em
[`spritesheet-grid.md`](spritesheet-grid.md), incluindo quantos frames tem
cada linha, contados visualmente.

## Passo 3 — Volta ao overlay (tecla `A`)

Janela volta a 240x240 com o sprite animado, sem grade e sem texto de ajuda.

**Aprovado se:** transição limpa, sem sobra de conteúdo do modo grade.

## Passo 4 — Click-through (tecla `C`)

Posicionar a janela sobre um botão clicável de outro app e apertar `C`.

**Aprovado se:** o clique atravessa e chega ao app por baixo.

**Você não fica preso.** O click-through se desliga sozinho por dois
mecanismos independentes: ao perder o foco (o próprio clique que atravessou
já dispara isso) e, como rede de segurança, por timer de 10 segundos. No modo
grade a contagem regressiva aparece no texto de ajuda; no modo overlay nada
aparece, para não sujar a verificação de transparência.

**Aprovado também se:** poucos segundos depois, a janela volta a responder a
clique e teclado sem precisar matar o processo.

## Passo 5 — Encerramento (tecla `Esc`)

**Aprovado se:** o app fecha e `spike-overlay.exe` não fica rodando.

---

## Se o Passo 1 reprovar

Registrar o resultado, o modelo da GPU e a versão do WebView2, e acionar o
**Plano B** do spec: janela opaca com cantos arredondados e moldura
estilizada. Perde a magia de o mascote flutuar sobre o desktop, mas mantém o
produto viável.

Antes de concluir pelo Plano B, vale testar em outra máquina se houver uma
à mão — transparência de WebView2 sobre desktop vivo tem casos de borda
conhecidos ligados a combinações específicas de GPU e driver, e o resultado
pode não se repetir.

---

## Limitação técnica conhecida do spike

`transparent` não tem setter em tempo de execução no Tauri v2. O modo grade
simula opacidade cobrindo a janela com CSS sólido, em vez de alternar a
propriedade real. Isso não afeta o que o gate mede: a transparência
verdadeira é exatamente o que o modo overlay exercita.
