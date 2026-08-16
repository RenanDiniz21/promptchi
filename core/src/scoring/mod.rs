pub mod tipo;
pub mod sinais;
pub mod rubrica;

pub use tipo::{classificar, ContextoSessao, TipoPrompt};
pub use sinais::{extrair, Sinais};
pub use rubrica::{avaliar, Dimensoes, TETO_CONTINUACAO};
