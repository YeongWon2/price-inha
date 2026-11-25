//! (35차 1 · 2 · 4번, 결정 19번 · P5 · P8) 금지 종목 · 업종표: 보관 폴더의 [`RULES_CONFIG_FILE`]을 읽고 쓰고,
//! 주문 전 점검이 쓰는 값을 쥔다. 값 검사(파일 · `POST` 같은 규칙)는 여기서 한다.
//!
//! - 켤 때 파일이 있으면 그 값이 조립 값([`crate::TradingWiring::rules`])을 대신한다([`load`]).
//! - 고친 값은 원자적으로 저장한 뒤(임시 파일 → 옮기기) 쥔 값을 바꾼다 — 다음 점검부터 쓰인다.
//! - 시연 실행(`App::launch_run` + `--demo`)이면 시연 업종을 따로 쥐고, 점검 때 업종이 없는 지켜보는 종목에만
//!   더한다(파일 · 읽기 응답에는 섞지 않는다, E56).

use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use pi_market::{Market, Symbol};
use pi_rules::RulesConfig;
use serde_json::{Map, Value, json};

use super::base::{exec_text, millis};
use super::{KIND_RULES_CONFIG, RULES_CONFIG_FILE, RULES_SECTOR_MAX_CHARS, Trading};
use crate::{AppError, TradingError};

/// 주문 길이 쥔 금지 종목 · 업종표.
pub(crate) struct RulesHold {
    /// 저장된(또는 조립이 준) 값. 읽기 응답 · 파일과 같다.
    saved: RwLock<RulesConfig>,
    /// 시연 실행의 시연 업종(종목 → 업종). 시연 실행이 아니면 비어 있다.
    demo: OnceLock<HashMap<Symbol, String>>,
    /// 고치기(저장 → 바꾸기 → 이벤트)를 한 줄로 세운다.
    turn: tokio::sync::Mutex<()>,
}

impl RulesHold {
    pub(super) fn new(rules: RulesConfig) -> RulesHold {
        RulesHold {
            saved: RwLock::new(rules),
            demo: OnceLock::new(),
            turn: tokio::sync::Mutex::new(()),
        }
    }

    pub(super) fn saved(&self) -> RulesConfig {
        self.saved
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(super) fn replace(&self, rules: RulesConfig) {
        *self
            .saved
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = rules;
    }
}

/// 시연 업종 이름: 차례 0 → "시연 업종 A", 25 → "시연 업종 Z", 26 → "시연 업종 A2" …
fn demo_sector_name(index: usize) -> String {
    let letter = char::from(b'A' + u8::try_from(index % 26).unwrap_or(0));
    match index / 26 {
        0 => format!("시연 업종 {letter}"),
        round => format!("시연 업종 {letter}{}", round + 1),
    }
}

/// (35차 4번) 지켜보는 종목마다 시연 업종(차례대로 서로 다른 이름).
pub(crate) fn demo_sectors(symbols: &[Symbol]) -> HashMap<Symbol, String> {
    symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| (symbol.clone(), demo_sector_name(index)))
        .collect()
}

fn market_of(word: &str) -> Option<Market> {
    match word {
        "kr" => Some(Market::Kr),
        "us" => Some(Market::Us),
        _ => None,
    }
}

fn market_word(market: Market) -> &'static str {
    match market {
        Market::Us => "us",
        _ => "kr",
    }
}

fn symbol_of(value: &Value) -> Option<Symbol> {
    Symbol::parse(value.as_str()?.trim()).ok()
}

/// 업종 이름: 앞뒤 공백을 뗀 뒤 1자 이상 [`RULES_SECTOR_MAX_CHARS`]자 이하.
fn sector_of(value: &Value) -> Option<String> {
    let name = value.as_str()?.trim();
    let count = name.chars().count();
    (1..=RULES_SECTOR_MAX_CHARS)
        .contains(&count)
        .then(|| name.to_string())
}

/// 몸통이 꼭 그 두 칸(`banned` · `sectors`)만 가진 객체인가.
fn two_fields(value: &Value) -> Option<&Map<String, Value>> {
    let object = value.as_object()?;
    let known = object.keys().all(|k| k == "banned" || k == "sectors");
    (known && object.contains_key("banned") && object.contains_key("sectors")).then_some(object)
}

/// 금지 종목 목록(같은 (시장, 종목)이 두 번이면 안 된다).
fn banned_of(value: &Value) -> Option<Vec<(Market, Symbol)>> {
    let mut seen = HashSet::new();
    let mut banned = Vec::new();
    for row in value.as_array()? {
        let row = row.as_object()?;
        if row.keys().any(|k| k != "market" && k != "symbol") {
            return None;
        }
        let market = market_of(row.get("market")?.as_str()?)?;
        let symbol = symbol_of(row.get("symbol")?)?;
        if !seen.insert((market_word(market), symbol.clone())) {
            return None;
        }
        banned.push((market, symbol));
    }
    Some(banned)
}

/// 업종표 줄들(종목, 업종 값)을 검사해 표로(같은 종목이 두 번이면 안 된다).
fn sectors_of<'a>(
    rows: impl Iterator<Item = (&'a Value, &'a Value)>,
) -> Option<HashMap<Symbol, String>> {
    let mut sectors = HashMap::new();
    for (symbol, sector) in rows {
        let symbol = symbol_of(symbol)?;
        let sector = sector_of(sector)?;
        if sectors.insert(symbol, sector).is_some() {
            return None;
        }
    }
    Some(sectors)
}

/// 파일 모양 `{"banned": [...], "sectors": {"종목": "업종"}}`을 읽는다. 틀리면 없음.
pub(super) fn parse_file(bytes: &[u8]) -> Option<RulesConfig> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let object = two_fields(&value)?;
    let banned_symbols = banned_of(&object["banned"])?;
    let keys: Vec<Value> = object["sectors"]
        .as_object()?
        .keys()
        .map(|k| Value::String(k.clone()))
        .collect();
    let values = object["sectors"].as_object()?.values();
    let sectors = sectors_of(keys.iter().zip(values))?;
    Some(RulesConfig {
        banned_symbols,
        sectors,
    })
}

/// `POST` 몸통 `{"banned": [...], "sectors": [{"symbol", "sector"}]}`을 읽는다. 틀리면 없음.
pub(super) fn parse_request(value: &Value) -> Option<RulesConfig> {
    let object = two_fields(value)?;
    let banned_symbols = banned_of(&object["banned"])?;
    let mut rows = Vec::new();
    for row in object["sectors"].as_array()? {
        let row = row.as_object()?;
        if row.keys().any(|k| k != "symbol" && k != "sector") {
            return None;
        }
        rows.push((row.get("symbol")?, row.get("sector")?));
    }
    let sectors = sectors_of(rows.into_iter())?;
    Some(RulesConfig {
        banned_symbols,
        sectors,
    })
}

fn banned_json(rules: &RulesConfig) -> Vec<Value> {
    rules
        .banned_symbols
        .iter()
        .map(|(market, symbol)| json!({"market": market_word(*market), "symbol": symbol.as_str()}))
        .collect()
}

/// 업종표 줄(종목 차례).
fn sorted_sectors(rules: &RulesConfig) -> Vec<(&Symbol, &String)> {
    let mut rows: Vec<_> = rules.sectors.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    rows
}

/// 파일에 쓸 모양.
fn file_json(rules: &RulesConfig) -> Value {
    let sectors: Map<String, Value> = sorted_sectors(rules)
        .into_iter()
        .map(|(symbol, sector)| (symbol.as_str().to_string(), json!(sector)))
        .collect();
    json!({"banned": banned_json(rules), "sectors": sectors})
}

/// 켤 때 보관 폴더의 [`RULES_CONFIG_FILE`]을 읽는다. 없으면 없음. 읽지 못하거나 깨졌으면
/// [`AppError::BadFile`](파일 이름과 종류만 — 값은 싣지 않는다).
pub(crate) fn load(home: &Path) -> Result<Option<RulesConfig>, AppError> {
    let bad = |kind| AppError::BadFile {
        file: RULES_CONFIG_FILE,
        kind,
    };
    let bytes = match std::fs::read(home.join(RULES_CONFIG_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(bad(error.kind())),
    };
    parse_file(&bytes)
        .map(Some)
        .ok_or_else(|| bad(ErrorKind::InvalidData))
}

/// 원자적으로 쓴다: 같은 폴더의 새 임시 파일에 쓰고 디스크에 내린 뒤 이름을 바꾼다(임시 파일은 남기지 않는다).
pub(super) fn save(home: &Path, rules: &RulesConfig) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let text = file_json(rules).to_string();
    let temp = home.join(format!(
        ".{RULES_CONFIG_FILE}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, home.join(RULES_CONFIG_FILE))
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

impl Trading {
    /// (35차 4번) 시연 실행의 시연 업종을 붙인다(한 번만). 점검 때 업종이 없는 지켜보는 종목에만 쓰인다.
    pub(crate) fn keep_demo_sectors(&self, sectors: Option<HashMap<Symbol, String>>) {
        if let Some(sectors) = sectors {
            let _ = self.rules.demo.set(sectors);
        }
    }

    /// 주문 전 점검이 지금 쓰는 금지 종목 · 업종표: 저장된 값에, 시연 실행이면 업종이 없는 종목의 시연 업종.
    pub(super) fn rules_now(&self) -> RulesConfig {
        let mut rules = self.rules.saved();
        let Some(demo) = self.rules.demo.get() else {
            return rules;
        };
        for (symbol, sector) in demo {
            rules
                .sectors
                .entry(symbol.clone())
                .or_insert_with(|| sector.clone());
        }
        for (index, symbol) in self.desk.symbols().into_iter().enumerate() {
            rules
                .sectors
                .entry(symbol)
                .or_insert_with(|| demo_sector_name(index));
        }
        rules
    }

    /// (35차 2번) 검사를 지난 값으로 통째 바꾼다: 원자적으로 저장 → 쥔 값을 바꿈(다음 점검부터) → 바꾼 일
    /// 이벤트([`KIND_RULES_CONFIG`]). 바꾼 뒤의 읽기 응답을 돌려준다. 저장하지 못하면 아무것도 바꾸지 않는다.
    pub(super) async fn change_rules(&self, rules: RulesConfig) -> Result<Value, TradingError> {
        let _turn = self.rules.turn.lock().await;
        save(self.desk.home(), &rules).map_err(|error| exec_text(error.kind()))?;
        let payload = json!({
            "at": millis(self.clock.now()).0,
            "by": "local",
            "banned": rules.banned_symbols.len(),
            "sectors": rules.sectors.len(),
        });
        self.rules.replace(rules);
        // 이미 저장하고 바꿨다 — 이벤트를 남기지 못해도 바꾼 값은 그대로다.
        if let Err(error) = self.record(KIND_RULES_CONFIG, payload).await {
            tracing::warn!(%error, "금지 종목 · 업종표를 바꾼 일을 남기지 못했습니다");
        }
        Ok(self.rules_view(true))
    }

    /// 읽기 응답([`super::RULES_CONFIG_PATH`]의 `GET` 모양).
    pub(super) fn rules_view(&self, editable: bool) -> Value {
        let saved = self.rules.saved();
        let now = self.rules_now();
        let sectors: Vec<Value> = sorted_sectors(&saved)
            .into_iter()
            .map(|(symbol, sector)| json!({"symbol": symbol.as_str(), "sector": sector}))
            .collect();
        let unsectored: Vec<String> = self
            .desk
            .symbols()
            .into_iter()
            .filter(|symbol| !now.sectors.contains_key(symbol))
            .map(|symbol| symbol.as_str().to_string())
            .collect();
        json!({
            "banned": banned_json(&saved),
            "sectors": sectors,
            "editable": editable,
            "unsectored": unsectored,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 시연_업종_이름은_차례마다_다르다() {
        assert_eq!(demo_sector_name(0), "시연 업종 A");
        assert_eq!(demo_sector_name(25), "시연 업종 Z");
        assert_eq!(demo_sector_name(26), "시연 업종 A2");
    }

    #[test]
    fn 파일_모양을_읽고_틀린_것은_거른다() {
        let ok = r#"{"banned": [{"market": "us", "symbol": "TSLA"}], "sectors": {"000660": " 반도체 "}}"#;
        let rules = parse_file(ok.as_bytes()).expect("맞는 모양");
        assert_eq!(
            rules.banned_symbols,
            vec![(Market::Us, Symbol::new("TSLA"))]
        );
        assert_eq!(rules.sectors[&Symbol::new("000660")], "반도체");
        assert!(parse_file(br#"{"banned": [], "sectors": {}, "x": 1}"#).is_none());
        assert!(parse_file(br#"{"banned": []}"#).is_none());
        let again = file_json(&rules).to_string();
        assert_eq!(parse_file(again.as_bytes()), Some(rules));
    }
}
