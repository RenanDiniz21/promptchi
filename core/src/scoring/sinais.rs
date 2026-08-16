/// Extensões de arquivo que contam como âncora concreta.
const EXTENSOES: [&str; 14] = [
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".cs", ".java", ".go", ".json", ".md", ".yml",
    ".yaml", ".toml",
];

/// Marcadores de restrição explícita.
const RESTRICOES: [&str; 10] = [
    "sem ", "não use", "nao use", "evite", "mantenha", "apenas ", "somente ", "no máximo",
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

pub fn extrair(texto: &str) -> Sinais {
    let baixo = texto.to_lowercase();
    let palavras: Vec<&str> = texto.split_whitespace().collect();

    let ancoras = palavras
        .iter()
        .filter(|p| e_ancora(p))
        .count();

    let restricoes = RESTRICOES.iter().filter(|m| baixo.contains(**m)).count();
    let formato_pedido = FORMATOS.iter().any(|m| baixo.contains(*m));
    let ruido = RUIDO.iter().filter(|m| baixo.contains(**m)).count();

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
    let baixo = palavra.to_lowercase();
    if EXTENSOES.iter().any(|e| baixo.ends_with(e) || baixo.contains(&format!("{e}:"))) {
        return true;
    }
    if palavra.contains('/') || palavra.contains('\\') {
        return true;
    }
    // camelCase / PascalCase: tem minúscula e uma maiúscula depois da primeira posição.
    let tem_minuscula = palavra.chars().any(|c| c.is_lowercase());
    let maiuscula_interna = palavra.chars().skip(1).any(|c| c.is_uppercase());
    palavra.len() > 3 && tem_minuscula && maiuscula_interna
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
    fn detecta_restricoes() {
        assert!(extrair("faz isso sem usar regex").restricoes >= 1);
        assert!(extrair("mantenha a assinatura atual").restricoes >= 1);
        assert_eq!(extrair("cria um endpoint").restricoes, 0);
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
        assert!(extrair("por favor, se possivel, obrigado").ruido >= 2);
        assert_eq!(extrair("corrige o teste").ruido, 0);
    }

    #[test]
    fn texto_vazio_nao_causa_panic() {
        let s = extrair("");
        assert_eq!(s.palavras, 0);
        assert_eq!(s.ancoras, 0);
        assert!(!s.formato_pedido);
    }
}
