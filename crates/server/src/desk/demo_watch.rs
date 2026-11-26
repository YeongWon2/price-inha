//! (다듬기 E 1번) 시연 기본 종목: 키도 종목도 없는 시연(실행 파일의 `--demo` 조립)은 시연용 기본 종목을 지켜본다.
//! 조립 값이라 보관 폴더에 쓰지 않는다(E56). 출처는 [`SymbolsSource::Demo`]이고 종목 등록 항목은 등록된 것이 아니다.
//! 사람이 종목을 등록하면 그것으로 바뀐다(출처 `saved`).

use pi_market::Symbol;

use super::Desk;
use crate::setup::SymbolsSource;

impl Desk {
    /// 지켜보는 종목이 없으면(인자 · 저장 모두 없음) `symbols`를 시연 기본 종목으로 쓴다. 이미 있으면 그대로다.
    /// 시연이 아니면 아무것도 하지 않는다. 띄울 때 시세 흐름이 돌기 전에 부른다(깨우지 않는다).
    pub(crate) fn use_demo_symbols(&self, symbols: Vec<Symbol>) {
        if !self.demo || symbols.is_empty() {
            return;
        }
        let mut watching = self.watching();
        if watching.source != SymbolsSource::None || !watching.symbols.is_empty() {
            return;
        }
        let shown: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
        tracing::info!(
            symbols = %shown.join(","),
            "지켜볼 종목이 없어 시연 기본 종목을 지켜봅니다(파일에 쓰지 않습니다)"
        );
        watching.symbols = symbols;
        watching.source = SymbolsSource::Demo;
    }
}
