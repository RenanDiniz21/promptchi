# Grade do spritesheet — kuriboh-alado

**Confirmada visualmente em 2026-08-15**, no modo grade do spike de overlay.
As linhas caem nas bordas dos frames sem ajuste — a hipótese do plano estava
correta e nenhuma constante precisou ser alterada.

- Imagem: **1536 x 2288**
- Célula: **192 x 208**
- Colunas: **8**   Linhas: **11**
- Linhas caem nas bordas dos frames: **sim**
- Ajuste necessário: **nenhum**
- Animação idle (linha 1, 7 frames, 1.05s): sem costura visível no loop

## Frames por linha

Contagem estimada por leitura da imagem, **ainda não conferida frame a frame**
no modo grade. Use como ponto de partida ao mapear os estados de animação; o
número de frames de uma linha só vira contratual quando aquele estado for
implementado de fato.

| Linha | Frames | Estado sugerido |
|---|---|---|
| 1 | 7 | `idle` + piscar — **confirmado em uso** |
| 2 | 8 | voo lateral, direção A |
| 3 | 8 | voo lateral, direção B |
| 4 | 4 | flutuar, asas abertas |
| 5 | 5 | asas abertas, variações → `analyzing` |
| 6 | 8 | tristeza, olhos marejados, sono → `disappointed`, `sleeping` |
| 7 | 6 | surpresa, boca aberta → `notice`, mastigação |
| 8 | 6 | agitação, asas batendo → `celebrating` |
| 9 | 6 | conjunto de expressões: alegre, bravo, choroso |
| 10 | 8 | loop de voo pequeno A |
| 11 | 8 | loop de voo pequeno B |

## Lacunas conhecidas

**Não existe animação de comer.** É a batida mais importante do produto — o
momento da recompensa. Composição prevista no spec: frames de boca aberta da
linha 7 como mastigação, sprite de comida em tween até a boca, bounce de
escala, corte para `celebrating` ou `disappointed`.

**Não existem sprites de comida.** No MVP são emoji renderizados como texto.

Onde os valores vivem: variáveis CSS `--cell-w`, `--cell-h`, `--idle-frames`
em [`spike-overlay/src/styles.css`](../../../spike-overlay/src/styles.css).
