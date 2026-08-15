use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use promptchi_core::types::Provider;

/// Quantos prompts recentes o conjunto lembra antes de esquecer o mais
/// antigo.
///
/// O binário roda o dia inteiro; sem teto, o conjunto cresceria para sempre.
/// 4096 vem da medição do corpus real: a sessão mais longa do Codex tem 158
/// `user_message` e o histórico inteiro da máquina, acumulado em 71 dias,
/// tem 1762. A janela precisa cobrir o histórico da maior sessão-pai que
/// pode ser reemitido de uma vez — 4096 é 26 vezes isso, custando algo como
/// 100 KB de memória.
pub const CAPACIDADE_DEDUP: usize = 4096;

/// Conjunto FIFO com teto dos prompts já emitidos, para o replay de fork do
/// Codex não sair duas vezes.
///
/// Existe por causa do fork de sessão: 51 dos 206 arquivos do corpus têm
/// `forked_from_id` e abrem reemitindo o histórico da sessão-pai. Como o
/// arquivo forkado carrega um id novo, o cursor dele nasce legitimamente em
/// zero — a duplicação só dá para barrar olhando o conteúdo.
///
/// **Quem consulta este conjunto importa tanto quanto o conjunto.** Ele é
/// alimentado por todo prompt do Codex, mas só barra prompt de sessão
/// forkada (ver `Estado::processar`). Medição sobre o corpus real, com o
/// filtro de subagente já aplicado, mostra por quê: barrar todo mundo
/// descartaria 155 prompts humanos legítimos — "yes", "Sim", "prossiga com
/// a implementação", repetidos ao longo do dia — para evitar 6 duplicatas
/// reais. Cerca de 25 prompts perdidos por duplicado evitado. Alimentar sem
/// barrar é o que faz o replay do fork encontrar os prompts originais da
/// sessão-pai, que não é forkada.
///
/// A chave é o texto do prompt combinado com o provider. Não entra
/// timestamp: medição sobre os forks reais mostra que o replay reescreve o
/// timestamp de cada linha (nenhum par (timestamp, texto) se repete entre
/// pai e fork), então incluí-lo anularia a deduplicação. Não entra sessão: o
/// replay é cross-sessão por definição, então incluí-la anularia o benefício
/// e ainda manteria a perda intra-sessão.
///
/// Guardamos hash de 64 bits, não o texto: memória constante por entrada,
/// independente do tamanho do prompt.
pub struct ConjuntoDeDuplicados {
    capacidade: usize,
    vistos: HashSet<u64>,
    ordem: VecDeque<u64>,
}

impl ConjuntoDeDuplicados {
    pub fn new(capacidade: usize) -> Self {
        Self {
            capacidade: capacidade.max(1),
            vistos: HashSet::new(),
            ordem: VecDeque::new(),
        }
    }

    /// Registra o prompt. Devolve `true` se ele é novo (deve ser emitido) e
    /// `false` se já passou por aqui dentro da janela.
    pub fn registrar(&mut self, provider: Provider, texto: &str) -> bool {
        let chave = chave(provider, texto);
        if !self.vistos.insert(chave) {
            return false;
        }
        self.ordem.push_back(chave);
        while self.ordem.len() > self.capacidade {
            if let Some(antigo) = self.ordem.pop_front() {
                self.vistos.remove(&antigo);
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.ordem.len()
    }
}

fn chave(provider: Provider, texto: &str) -> u64 {
    let mut h = DefaultHasher::new();
    provider.as_str().hash(&mut h);
    texto.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesmo_texto_so_e_emitido_na_primeira_vez() {
        let mut c = ConjuntoDeDuplicados::new(CAPACIDADE_DEDUP);
        assert!(c.registrar(Provider::Codex, "arruma o worker de refugo"));
        assert!(!c.registrar(Provider::Codex, "arruma o worker de refugo"));
        assert!(!c.registrar(Provider::Codex, "arruma o worker de refugo"));
    }

    #[test]
    fn textos_diferentes_passam() {
        let mut c = ConjuntoDeDuplicados::new(CAPACIDADE_DEDUP);
        assert!(c.registrar(Provider::Codex, "um"));
        assert!(c.registrar(Provider::Codex, "dois"));
    }

    /// O conjunto em si é agnóstico de provider — quem decide que só o
    /// Codex passa por aqui é `Estado::processar`, e isso é testado lá.
    #[test]
    fn provider_faz_parte_da_chave() {
        let mut c = ConjuntoDeDuplicados::new(CAPACIDADE_DEDUP);
        assert!(c.registrar(Provider::Codex, "mesmo texto"));
        assert!(c.registrar(Provider::ClaudeCode, "mesmo texto"));
    }

    #[test]
    fn rajada_de_replay_de_fork_e_barrada_inteira() {
        // Simula o que o Codex faz ao forkar: a sessão nova reabre com todo
        // o histórico da sessão-pai.
        let historico = ["primeiro", "segundo", "terceiro", "quarto"];
        let mut c = ConjuntoDeDuplicados::new(CAPACIDADE_DEDUP);
        for p in historico {
            assert!(c.registrar(Provider::Codex, p));
        }
        for p in historico {
            assert!(!c.registrar(Provider::Codex, p), "replay de fork vazou: {p}");
        }
        // O prompt novo, digitado depois do fork, passa.
        assert!(c.registrar(Provider::Codex, "quinto, esse é novo"));
    }

    #[test]
    fn conjunto_respeita_o_teto_descartando_o_mais_antigo() {
        let mut c = ConjuntoDeDuplicados::new(3);
        for p in ["a", "b", "c", "d", "e"] {
            assert!(c.registrar(Provider::Codex, p));
        }
        assert_eq!(c.len(), 3, "o conjunto nao pode crescer sem limite");
        // "a" e "b" sairam da janela: voltam a ser considerados novos.
        assert!(c.registrar(Provider::Codex, "a"));
        // "e" continua na janela.
        assert!(!c.registrar(Provider::Codex, "e"));
    }

    #[test]
    fn teto_zero_nao_causa_panic() {
        let mut c = ConjuntoDeDuplicados::new(0);
        assert!(c.registrar(Provider::Codex, "a"));
        assert!(c.registrar(Provider::Codex, "b"));
        assert_eq!(c.len(), 1);
    }
}
