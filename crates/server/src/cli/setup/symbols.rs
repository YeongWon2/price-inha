//! 종목 구역: 쉼표로 나눈 종목 한 줄을 도는 서버의 [`crate::SETUP_SYMBOLS_PATH`]로 등록한다(검사는 서버의
//! 것 그대로). 빈 줄은 건너뛰기, 거절되면 까닭 뒤 다시 묻는다. 저장한 종목 가운데 업종표에 없는 것이 있으면
//! (도는 서버의 [`crate::RULES_CONFIG_PATH`]가 알리는 `unsectored`) [`super::SETUP_SECTOR_NOTICE`] 한 줄.

use serde_json::{Value, json};

use super::SETUP_SECTOR_NOTICE;
use super::prompt::Prompter;
use super::server::{Link, Reply, gives_up};
use super::summary::Outcome;
use crate::RULES_CONFIG_PATH;
use crate::setup::SETUP_SYMBOLS_PATH;

/// 종목 구역을 돈다.
pub(crate) async fn symbols(link: &Link, prompter: &mut Prompter<'_>) -> Outcome {
    prompter.title("지켜볼 종목");
    loop {
        let Some(line) =
            prompter.line("종목 코드를 쉼표로 나눠 넣으세요(예: 005930,000660, 빈 줄 = 건너뛰기):")
        else {
            return Outcome::ended();
        };
        let list = split(&line);
        if list.is_empty() {
            return Outcome::skipped("넣지 않음");
        }
        let body = json!({ "symbols": list }).to_string().into_bytes();
        match link.post(SETUP_SYMBOLS_PATH, body).await {
            Reply::Accepted => {
                prompter.say(&format!("지켜볼 종목을 저장했습니다: {}", list.join(", ")));
                if unsectored(link).await {
                    prompter.say(SETUP_SECTOR_NOTICE);
                }
                return Outcome::done(&format!("{}개", list.len()));
            }
            Reply::Refused(refusal) => {
                prompter.say(&refusal.to_string());
                if gives_up(refusal) {
                    return Outcome::failed(&refusal.to_string());
                }
            }
            Reply::Broken => return Outcome::failed(super::BROKEN),
        }
    }
}

/// 쉼표로 나눈 종목들(앞뒤 공백을 떼고 빈 칸은 뺀다).
fn split(line: &str) -> Vec<String> {
    line.split(',')
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
        .map(str::to_string)
        .collect()
}

/// 지켜보는 종목(방금 저장한 것) 가운데 업종표에 없는 것이 있는가(도는 서버가 아는 것). 읽지 못하면 거짓.
async fn unsectored(link: &Link) -> bool {
    let Some(config) = link.get(RULES_CONFIG_PATH).await else {
        return false;
    };
    config
        .get("unsectored")
        .and_then(Value::as_array)
        .is_some_and(|list| !list.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 쉼표로_나누고_빈_칸은_뺀다() {
        assert_eq!(split(" 005930, ,000660 ,"), ["005930", "000660"]);
        assert!(split("  ").is_empty());
    }
}
