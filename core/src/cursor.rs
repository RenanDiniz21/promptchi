use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::jsonl::LineBuffer;

/// Quantidade de bytes iniciais usada como "impressão digital" do arquivo,
/// para detectar substituição por outro arquivo sem depender de metadados
/// específicos de sistema operacional (inode, file index, etc.).
const TAMANHO_PREFIXO: usize = 32;

/// Lê incrementalmente um arquivo append-only, emitindo apenas linhas completas.
pub struct FileCursor {
    path: PathBuf,
    offset: u64,
    buffer: LineBuffer,
    prefixo: Vec<u8>,
}

impl FileCursor {
    pub fn new(path: PathBuf) -> Self {
        Self { path, offset: 0, buffer: LineBuffer::new(), prefixo: Vec::new() }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Substitui o caminho observado, preservando `offset`, `buffer` e
    /// `prefixo`. Usado quando o processo escritor move/renomeia o arquivo
    /// (ex.: o Codex move sessões de `sessions/` para `archived_sessions/`)
    /// sem alterar o conteúdo: como a impressão digital do prefixo continua
    /// batendo, a leitura prossegue de onde parou em vez de reemitir o
    /// arquivo inteiro sob o caminho novo.
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    pub fn read_new(&mut self) -> std::io::Result<Vec<String>> {
        // O arquivo pode sumir a qualquer momento nesta função (corrida com
        // o processo escritor apagando/rotacionando o arquivo). Em todos os
        // pontos de I/O abaixo, `NotFound` degrada em silêncio para lista
        // vazia; qualquer outro erro de I/O propaga normalmente.
        macro_rules! ou_vazio {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(Vec::new())
                    }
                    Err(e) => return Err(e),
                }
            };
        }

        let mut f = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        // Impressão digital do início do arquivo: se os bytes iniciais
        // mudaram em relação à última leitura, o arquivo foi substituído por
        // outro — mesmo que o tamanho seja igual ou maior (o que o
        // encolhimento sozinho não detecta). Comparar apenas o prefixo comum
        // evita falso positivo em arquivo que só cresceu (bytes iniciais
        // continuam iguais) e em arquivo vazio (n = 0).
        ou_vazio!(f.seek(SeekFrom::Start(0)));
        let mut atual = Vec::new();
        ou_vazio!((&mut f).take(TAMANHO_PREFIXO as u64).read_to_end(&mut atual));
        let n = self.prefixo.len().min(atual.len());
        if self.prefixo[..n] != atual[..n] {
            self.offset = 0;
            self.buffer.clear();
        }
        self.prefixo = atual;

        let tamanho = ou_vazio!(f.metadata()).len();

        // Arquivo encolheu: foi truncado. Recomeçar.
        if tamanho < self.offset {
            self.offset = 0;
            self.buffer.clear();
        }
        if tamanho == self.offset {
            return Ok(Vec::new());
        }

        ou_vazio!(f.seek(SeekFrom::Start(self.offset)));
        let mut bytes = Vec::new();
        let lidos = ou_vazio!(f.read_to_end(&mut bytes)) as u64;
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
    fn arquivo_substituido_por_maior_e_detectado() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "abc");

        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        std::fs::write(&p, "wxyz\n").unwrap();
        assert_eq!(c.read_new().unwrap(), vec!["wxyz".to_string()]);
    }

    #[test]
    fn arquivo_substituido_por_mesmo_tamanho_e_detectado() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        escrever(&p, "aaa\n");

        let mut c = FileCursor::new(p.clone());
        c.read_new().unwrap();

        std::fs::write(&p, "bbb\n").unwrap();
        assert_eq!(c.read_new().unwrap(), vec!["bbb".to_string()]);
    }

    #[test]
    fn arquivo_inexistente_retorna_vazio_sem_erro() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = FileCursor::new(dir.path().join("nao_existe.jsonl"));
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn set_path_preserva_estado_e_evita_duplicacao_ao_mover_arquivo() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jsonl");
        escrever(&a, "um\n");

        let mut c = FileCursor::new(a.clone());
        assert_eq!(c.read_new().unwrap(), vec!["um".to_string()]);

        // Simula o Codex movendo o arquivo: mesmo conteúdo, caminho novo.
        let b = dir.path().join("b.jsonl");
        std::fs::copy(&a, &b).unwrap();
        c.set_path(b);

        // Nada é reemitido: offset e prefixo preservados fazem a leitura
        // prosseguir de onde parou, não recomeçar do zero.
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());
    }
}
