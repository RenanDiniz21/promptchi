/// Acumula bytes lidos e emite apenas linhas terminadas em `\n`.
/// A linha final incompleta fica retida até o próximo `push`.
#[derive(Debug, Default)]
pub struct LineBuffer {
    pending: String,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self { pending: String::new() }
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);
        let mut out = Vec::new();
        while let Some(idx) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=idx).collect();
            let trimmed = line.trim_end_matches(['\n', '\r']);
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linha_incompleta_nao_e_emitida() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("{\"a\":1"), Vec::<String>::new());
    }

    #[test]
    fn linha_completa_no_chunk_seguinte() {
        let mut b = LineBuffer::new();
        b.push("{\"a\":1");
        assert_eq!(b.push("}\n"), vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn multiplas_linhas_em_um_chunk() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("um\ndois\ntres"), vec!["um".to_string(), "dois".to_string()]);
    }

    #[test]
    fn crlf_e_removido() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("um\r\n"), vec!["um".to_string()]);
    }

    #[test]
    fn linhas_vazias_sao_ignoradas() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("\n\num\n"), vec!["um".to_string()]);
    }

    #[test]
    fn utf8_multibyte_nao_quebra() {
        let mut b = LineBuffer::new();
        assert_eq!(b.push("olá çãé\n"), vec!["olá çãé".to_string()]);
    }

    #[test]
    fn clear_descarta_pendente() {
        let mut b = LineBuffer::new();
        b.push("parcial");
        b.clear();
        assert_eq!(b.push("completa\n"), vec!["completa".to_string()]);
    }
}
