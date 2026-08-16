/// Extensões de arquivo que contam como âncora concreta.
const EXTENSOES: [&str; 14] = [
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".cs", ".java", ".go", ".json", ".md", ".yml",
    ".yaml", ".toml",
];

/// Marcadores de restrição explícita.
const RESTRICOES: [&str; 10] = [
    "sem", "não use", "nao use", "evite", "mantenha", "apenas", "somente", "no máximo",
    "no maximo", "em vez de",
];

/// Marcadores de formato de saída pedido.
const FORMATOS: [&str; 9] = [
    "em json", "em yaml", "em markdown", "em tabela", "uma tabela", "em lista", "só o código",
    "so o codigo", "apenas o codigo",
];

/// Pronomes sem antecedente explícito.
const DEITICOS: [&str; 8] = ["isso", "isto", "aquilo", "esse", "essa", "aquele", "aquela", "ele"];

/// Cortesia que não carrega informação.
const RUIDO: [&str; 6] = ["por favor", "se possivel", "se possível", "obrigado", "valeu", "please"];

/// Só o que parsing de fato enxerga. Nenhum julgamento semântico aqui.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sinais {
    pub palavras: usize,
    pub ancoras: usize,
    pub restricoes: usize,
    pub formato_pedido: bool,
    pub deiticos: usize,
    pub ruido: usize,
}

/// Verifica se uma frase está cercada por fronteira de palavra (não-alfanumérico ou fim/início).
fn tem_fronteira_palavra(texto: &str, frase: &str) -> bool {
    let baixo = texto.to_lowercase();
    let frase_lower = frase.to_lowercase();
    let mut pos = 0;

    while let Some(idx) = baixo[pos..].find(&frase_lower) {
        let start = pos + idx;
        let end = start + frase_lower.len();

        // Verifica fronteira usando fatias de bytes, respeitando UTF-8
        let antes_ok = baixo[..start].chars().next_back().map_or(true, |c| !c.is_alphanumeric());
        let depois_ok = baixo[end..].chars().next().map_or(true, |c| !c.is_alphanumeric());

        if antes_ok && depois_ok {
            return true;
        }

        // Avança pelo tamanho em bytes do primeiro caractere de fato casado,
        // não um literal 1: se a frase casada começar com caractere
        // multibyte (á, é, ã, ç), pos+1 cai no meio do caractere e
        // baixo[pos..] na próxima volta panica por fronteira UTF-8 inválida.
        pos = start + baixo[start..].chars().next().map_or(1, |c| c.len_utf8());
    }

    false
}

pub fn extrair(texto: &str) -> Sinais {
    let palavras: Vec<&str> = texto.split_whitespace().collect();

    let ancoras = palavras
        .iter()
        .filter(|p| e_ancora(p))
        .count();

    let restricoes = RESTRICOES.iter().filter(|m| tem_fronteira_palavra(texto, m)).count();
    let formato_pedido = FORMATOS.iter().any(|m| tem_fronteira_palavra(texto, m));
    let ruido = RUIDO.iter().filter(|m| tem_fronteira_palavra(texto, m)).count();

    let deiticos = palavras
        .iter()
        .filter(|p| {
            let limpo = p.to_lowercase();
            let limpo_ref = limpo.trim_matches(|c: char| !c.is_alphanumeric());
            DEITICOS.iter().any(|d| *d == limpo_ref)
        })
        .count();

    Sinais { palavras: palavras.len(), ancoras, restricoes, formato_pedido, deiticos, ruido }
}

/// Âncora é referência concreta: caminho, arquivo com extensão conhecida, ou
/// identificador em camelCase/PascalCase. Prosa comum não produz âncora — no
/// corpus real só um terço dos prompts tem alguma.
fn e_ancora(palavra: &str) -> bool {
    // Apara pontuação do fim para testar extensão de arquivo
    let limpo = palavra.trim_end_matches(|c: char| !c.is_alphanumeric());
    let baixo = limpo.to_lowercase();

    if EXTENSOES.iter().any(|e| baixo.ends_with(e) || baixo.contains(&format!("{e}:"))) {
        return true;
    }

    // Barra só conta como âncora se parecer caminho de verdade. Extensão
    // conhecida já foi checada acima; aqui basta um segmento com 3+
    // caracteres que não seja puramente numérico. Sem isso, data (24/08) e
    // conjunção (e/ou, ele/ela) — comuns em português — viravam âncora falsa.
    if palavra.contains('/') || palavra.contains('\\') {
        let parece_caminho = palavra.split(|c| c == '/' || c == '\\').any(|seg| {
            let seg = seg.trim_matches(|c: char| !c.is_alphanumeric());
            seg.len() >= 3 && !seg.chars().all(|c| c.is_ascii_digit())
        });
        if parece_caminho {
            return true;
        }
    }

    // camelCase / PascalCase, checado por segmento separado por ponto: duas
    // frases coladas por ponto sem espaço ("bug.Depois") não podem casar
    // como se fossem um identificador só. Cada segmento precisa ter
    // minúscula e maiúscula interna por conta própria.
    limpo.split('.').any(e_camel_case)
}

/// Segmento com minúscula e uma maiúscula depois da primeira posição —
/// ex.: "parseLine". "Depois" (maiúscula só na posição 0) não conta.
fn e_camel_case(segmento: &str) -> bool {
    let tem_minuscula = segmento.chars().any(|c| c.is_lowercase());
    let maiuscula_interna = segmento.chars().skip(1).any(|c| c.is_uppercase());
    segmento.len() > 3 && tem_minuscula && maiuscula_interna
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_palavras() {
        assert_eq!(extrair("uma duas tres").palavras, 3);
        assert_eq!(extrair("").palavras, 0);
    }

    #[test]
    fn caminho_de_arquivo_e_ancora() {
        assert_eq!(extrair("corrige src/main.rs").ancoras, 1);
        assert_eq!(extrair("veja core\\src\\lib.rs agora").ancoras, 1);
    }

    #[test]
    fn identificador_camelcase_e_ancora() {
        assert_eq!(extrair("o metodo parseLine falhou").ancoras, 1);
    }

    #[test]
    fn texto_comum_nao_tem_ancora() {
        assert_eq!(extrair("melhora isso um pouco por favor").ancoras, 0);
    }

    #[test]
    fn ancora_com_pontuacao_final() {
        assert_eq!(extrair("veja main.rs.").ancoras, 1);
        assert_eq!(extrair("olha o core/src/lib.rs.").ancoras, 1);
    }

    #[test]
    fn detecta_restricoes() {
        assert_eq!(extrair("faz isso sem usar regex").restricoes, 1);
        assert_eq!(extrair("mantenha a assinatura atual").restricoes, 1);
        assert_eq!(extrair("cria um endpoint").restricoes, 0);
    }

    #[test]
    fn restricoes_sem_falso_positivo() {
        assert_eq!(extrair("mantenhamos os testes atuais").restricoes, 0);
        assert_eq!(extrair("nao evitem isso").restricoes, 0);
    }

    #[test]
    fn detecta_formato_pedido() {
        assert!(extrair("me devolve em json").formato_pedido);
        assert!(extrair("responde em uma tabela").formato_pedido);
        assert!(extrair("apenas o codigo, sem explicacao").formato_pedido);
        assert!(!extrair("corrige o bug").formato_pedido);
    }

    #[test]
    fn conta_deiticos() {
        assert_eq!(extrair("corrige isso").deiticos, 1);
        assert_eq!(extrair("muda isso e aquilo").deiticos, 2);
        assert_eq!(extrair("corrige src/main.rs").deiticos, 0);
    }

    #[test]
    fn conta_ruido_de_cortesia() {
        assert_eq!(extrair("por favor, se possivel, obrigado").ruido, 3);
        assert_eq!(extrair("corrige o teste").ruido, 0);
    }

    #[test]
    fn ruido_sem_falso_positivo() {
        assert_eq!(extrair("isso prevaleu no teste").ruido, 0);
    }

    #[test]
    fn texto_vazio_nao_causa_panic() {
        let s = extrair("");
        assert_eq!(s.palavras, 0);
        assert_eq!(s.ancoras, 0);
        assert_eq!(s.restricoes, 0);
        assert_eq!(s.formato_pedido, false);
        assert_eq!(s.deiticos, 0);
        assert_eq!(s.ruido, 0);
    }

    #[test]
    fn restricao_com_acentuacao_antes() {
        assert_eq!(extrair("não fiz sem querer").restricoes, 1);
    }

    #[test]
    fn ruido_com_acentuacao_antes() {
        assert_eq!(extrair("é isso, valeu").ruido, 1);
    }

    #[test]
    fn ruido_capitalizado() {
        assert_eq!(extrair("Por favor, ajuda com isso").ruido, 1);
    }

    #[test]
    fn restricao_capitalizada() {
        assert_eq!(extrair("Mantenha a assinatura atual").restricoes, 1);
    }

    #[test]
    fn falso_positivo_barrado_com_acentuacao() {
        assert_eq!(extrair("nós mantenhamos os testes").restricoes, 0);
    }

    // ACHADO 1: ponto sem espaço colava duas frases num token só, e o
    // pedaço depois do ponto (com maiúscula na primeira posição) passava
    // como camelCase.
    #[test]
    fn ponto_sem_espaco_nao_produz_ancora_falsa() {
        assert_eq!(extrair("Corrige o bug.Depois roda os testes").ancoras, 0);
    }

    #[test]
    fn camelcase_de_verdade_continua_ancora() {
        assert_eq!(extrair("o metodo parseLine falhou").ancoras, 1);
    }

    // ACHADO 2: barra fora de caminho (data, conjunção) virava âncora falsa.
    #[test]
    fn data_com_barra_nao_e_ancora() {
        assert_eq!(extrair("faz o deploy hoje 24/08").ancoras, 0);
    }

    #[test]
    fn conjuncao_com_barra_nao_e_ancora() {
        assert_eq!(extrair("usa e/ou conforme o caso").ancoras, 0);
    }

    #[test]
    fn caminho_com_extensao_continua_ancora() {
        assert_eq!(extrair("corrige src/main.rs").ancoras, 1);
    }

    #[test]
    fn diretorio_com_segmento_longo_e_ancora() {
        assert_eq!(extrair("move UserService para services/").ancoras, 2);
    }

    #[test]
    fn caminho_multi_segmento_com_extensao_e_ancora() {
        assert_eq!(extrair("volta o que voce fez em core/src/lib.rs").ancoras, 1);
    }

    // ACHADO 4: avançar por 1 byte literal, em vez do tamanho do caractere
    // casado, quebra fronteira UTF-8 quando a frase casada começa com
    // caractere multibyte que fica dentro de outra palavra (não é fronteira
    // válida, então o laço precisa avançar e tentar de novo). "café" contém
    // "é" colado a "f" (não é fronteira de palavra), forçando o laço a
    // avançar além do match; com pos = start + 1 isso caía no meio do "é"
    // (2 bytes em UTF-8) e baixo[pos..] entrava em pânico.
    #[test]
    fn fronteira_palavra_nao_panica_avancando_apos_char_multibyte() {
        assert!(!tem_fronteira_palavra("café com açúcar", "é"));
    }
}
