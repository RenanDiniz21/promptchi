use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::jsonl::LineBuffer;

/// Lê incrementalmente um arquivo append-only, emitindo apenas linhas completas.
pub struct FileCursor {
    path: PathBuf,
    offset: u64,
    buffer: LineBuffer,
}

impl FileCursor {
    pub fn new(path: PathBuf) -> Self {
        Self { path, offset: 0, buffer: LineBuffer::new() }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn read_new(&mut self) -> std::io::Result<Vec<String>> {
        let mut f = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let tamanho = f.metadata()?.len();

        // Arquivo encolheu: foi truncado ou substituído. Recomeçar.
        if tamanho < self.offset {
            self.offset = 0;
            self.buffer.clear();
        }
        if tamanho == self.offset {
            return Ok(Vec::new());
        }

        f.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        let lidos = f.read_to_end(&mut bytes)? as u64;
        self.offset += lidos;

        // Bytes inválidos não devem derrubar a captura.
        let texto = String::from_utf8_lossy(&bytes);
        Ok(self.buffer.push(&texto))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    fn escrever(path: &std::path::Path, conteudo: &str) {
        let mut f = OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(conteudo.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    #[test]
    fn le_apenas_o_que_chegou_desde_a_ultima_leitura() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "um\ndois\n");

        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), vec!["um".to_string(), "dois".to_string()]);
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        escrever(&p, "tres\n");
        assert_eq!(c.read_new().unwrap(), vec!["tres".to_string()]);
    }

    #[test]
    fn linha_parcial_e_retida_ate_completar() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "{\"a\":");

        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        escrever(&p, "1}\n");
        assert_eq!(c.read_new().unwrap(), vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn arquivo_truncado_reinicia_do_zero() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "antigo\n");

        let mut c = FileCursor::new(p.clone());
        c.read_new().unwrap();
        assert!(c.offset() > 0);

        std::fs::write(&p, "novo\n").unwrap();
        assert_eq!(c.read_new().unwrap(), vec!["novo".to_string()]);
    }

    #[test]
    fn arquivo_inexistente_retorna_vazio_sem_erro() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = FileCursor::new(dir.path().join("nao_existe.jsonl"));
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());
    }
}
