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
    /// Bytes lidos que ainda não formam UTF-8 válido. Uma leitura pode
    /// terminar no meio de uma sequência multibyte (o corpus é em português,
    /// onde isso é frequente); esses bytes ficam retidos aqui até o restante
    /// da sequência chegar na leitura seguinte. Converter com perdas neste
    /// ponto destruiria o caractere e, por tabela, a linha inteira.
    cauda: Vec<u8>,
}

impl FileCursor {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            buffer: LineBuffer::new(),
            prefixo: Vec::new(),
            cauda: Vec::new(),
        }
    }

    /// Descarta todo estado de leitura parcial. Usado quando o arquivo foi
    /// truncado ou substituído: o que estava retido pertence ao conteúdo
    /// antigo e não pode ser colado no novo.
    fn reiniciar(&mut self) {
        self.offset = 0;
        self.buffer.clear();
        self.cauda.clear();
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
            self.reiniciar();
        }
        self.prefixo = atual;

        let tamanho = ou_vazio!(f.metadata()).len();

        // Arquivo encolheu: foi truncado. Recomeçar.
        if tamanho < self.offset {
            self.reiniciar();
        }
        if tamanho == self.offset {
            return Ok(Vec::new());
        }

        ou_vazio!(f.seek(SeekFrom::Start(self.offset)));
        let mut bytes = Vec::new();
        let lidos = ou_vazio!(f.read_to_end(&mut bytes)) as u64;
        self.offset += lidos;

        Ok(self.decodificar(&bytes))
    }

    /// Entrega ao `LineBuffer` a maior porção dos bytes lidos que já é UTF-8
    /// válido, retendo o resto na cauda.
    ///
    /// O caso comum — cauda vazia, que é o de toda leitura que não terminou
    /// no meio de um caractere — valida os bytes lidos no lugar, sem cópia
    /// nenhuma. Só quando há cauda pendente (no máximo 3 bytes) é que vale
    /// concatenar. Concatenar sempre dobraria o pico de memória do arranque,
    /// que é dominado pelo maior arquivo isolado do corpus (124 MB hoje).
    fn decodificar(&mut self, bytes: &[u8]) -> Vec<String> {
        if self.cauda.is_empty() {
            let (saida, retido) = Self::fatiar(&mut self.buffer, bytes);
            self.cauda.extend_from_slice(&bytes[retido..]);
            return saida;
        }

        let mut pendente = std::mem::take(&mut self.cauda);
        pendente.extend_from_slice(bytes);
        let (saida, retido) = Self::fatiar(&mut self.buffer, &pendente);
        pendente.drain(..retido);
        self.cauda = pendente;
        saida
    }

    /// Empurra para `buffer` a maior porção válida de `bytes` e devolve o
    /// índice a partir do qual os bytes precisam ficar retidos.
    ///
    /// `Utf8Error` distingue os dois motivos de parada:
    /// - `error_len() == None`: sequência multibyte incompleta no fim do
    ///   pedaço lido. Os bytes ficam retidos para a próxima leitura.
    /// - `error_len() == Some(n)`: bytes genuinamente inválidos. São pulados,
    ///   porque retê-los travaria o cursor para sempre — a decodificação
    ///   nunca avançaria e todo o arquivo dali em diante seria perdido.
    fn fatiar(buffer: &mut LineBuffer, bytes: &[u8]) -> (Vec<String>, usize) {
        let mut saida = Vec::new();
        let mut inicio = 0usize;
        loop {
            match std::str::from_utf8(&bytes[inicio..]) {
                Ok(texto) => {
                    saida.extend(buffer.push(texto));
                    inicio = bytes.len();
                    break;
                }
                Err(e) => {
                    let ate = e.valid_up_to();
                    if ate > 0 {
                        // Já validado por `valid_up_to`; o `if let` evita
                        // qualquer possibilidade de panic mesmo assim.
                        if let Ok(texto) = std::str::from_utf8(&bytes[inicio..inicio + ate]) {
                            saida.extend(buffer.push(texto));
                        }
                    }
                    match e.error_len() {
                        None => {
                            inicio += ate;
                            break;
                        }
                        Some(ruins) => {
                            inicio += ate + ruins;
                        }
                    }
                }
            }
        }
        (saida, inicio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    fn escrever(path: &std::path::Path, conteudo: &str) {
        escrever_bytes(path, conteudo.as_bytes());
    }

    fn escrever_bytes(path: &std::path::Path, conteudo: &[u8]) {
        let mut f = OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(conteudo).unwrap();
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
    fn multibyte_partido_entre_duas_leituras_chega_integro() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");

        // "não" em UTF-8: o 'ã' são dois bytes (0xC3 0xA3). A primeira
        // leitura termina exatamente no meio dele.
        let linha = "{\"m\":\"não é açaí\"}\n";
        let bytes = linha.as_bytes();
        let corte = linha.find('ã').unwrap() + 1; // metade do 'ã'
        escrever_bytes(&p, &bytes[..corte]);

        let mut c = FileCursor::new(p.clone());
        assert_eq!(
            c.read_new().unwrap(),
            Vec::<String>::new(),
            "linha incompleta não pode ser emitida"
        );

        escrever_bytes(&p, &bytes[corte..]);
        assert_eq!(
            c.read_new().unwrap(),
            vec!["{\"m\":\"não é açaí\"}".to_string()],
            "o caractere partido entre leituras precisa chegar íntegro"
        );
    }

    #[test]
    fn multibyte_partido_nao_contamina_a_linha_seguinte() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");

        let a = "olá\n";
        let corte = a.find('á').unwrap() + 1;
        escrever_bytes(&p, &a.as_bytes()[..corte]);

        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        escrever_bytes(&p, &a.as_bytes()[corte..]);
        escrever_bytes(&p, "segunda\n".as_bytes());
        assert_eq!(
            c.read_new().unwrap(),
            vec!["olá".to_string(), "segunda".to_string()]
        );
    }

    #[test]
    fn bytes_invalidos_nao_travam_o_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");

        // 0xFF nunca é válido em UTF-8: `error_len()` devolve `Some`, e os
        // bytes ruins precisam ser pulados para a leitura seguir.
        escrever_bytes(&p, &[0xFF, 0xFE]);
        let mut c = FileCursor::new(p.clone());
        assert_eq!(c.read_new().unwrap(), Vec::<String>::new());

        escrever_bytes(&p, "boa\n".as_bytes());
        assert_eq!(c.read_new().unwrap(), vec!["boa".to_string()]);
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
