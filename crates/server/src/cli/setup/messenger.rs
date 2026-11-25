//! (39차) 메신저 구역(`setup messenger`): 봇 토큰(보이지 않는 입력) → 도는 서버의
//! [`crate::MESSENGER_TELEGRAM_PATH`]로 등록(서버가 getMe로 확인한다) → "봇에게 아무 말이나" → 코드 →
//! [`crate::MESSENGER_PAIRING_APPROVE_PATH`]. 도는 서버는 38차의 신원 확인을 거쳐 찾은 것이다([`Link`]).
//! 토큰은 출력 · 기록 어디에도 다시 찍지 않는다.

use serde_json::Value;

use super::prompt::Prompter;
use super::server::{Link, gives_up, refusal_of};
use super::summary::Outcome;
use super::{
    SETUP_MESSENGER_CODE_PROMPT, SETUP_MESSENGER_PAIRED, SETUP_MESSENGER_SEND_HINT,
    SETUP_MESSENGER_TOKEN_PROMPT,
};
use crate::{
    MESSENGER_LOCKED, MESSENGER_OWNER_EXISTS, MESSENGER_PAIRING_APPROVE_PATH,
    MESSENGER_TELEGRAM_PATH, MESSENGER_WRONG_CODE,
};

/// 토큰 단계의 끝.
enum Token {
    /// 등록됐다(봇 이름).
    Registered(String),
    /// 구역을 이 결과로 끝낸다.
    Done(Outcome),
}

/// 메신저 구역을 돈다.
pub(crate) async fn messenger(link: &Link, prompter: &mut Prompter<'_>) -> Outcome {
    prompter.title("메신저 (텔레그램)");
    prompter.say("토큰 글자는 보이지 않게 읽고, 저장하기 전에 서버가 텔레그램에 확인합니다");
    let bot = match token(link, prompter).await {
        Token::Registered(bot) => bot,
        Token::Done(outcome) => return outcome,
    };
    prompter.say(&format!("봇 @{bot}을(를) 확인하고 등록했습니다"));
    prompter.say(SETUP_MESSENGER_SEND_HINT);
    pair(link, prompter).await
}

/// 토큰 본문 `{"token": ..}`(중간 글자 없이 바로 본문에 쓴다).
fn token_body(token: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(token.len() * 6 + 16);
    body.extend_from_slice(b"{\"token\":");
    let _ = serde_json::to_writer(&mut body, token);
    body.push(b'}');
    body
}

async fn token(link: &Link, prompter: &mut Prompter<'_>) -> Token {
    loop {
        let Some(token) = prompter.secret(SETUP_MESSENGER_TOKEN_PROMPT) else {
            return Token::Done(Outcome::ended());
        };
        if token.is_empty() {
            return Token::Done(Outcome::skipped("넣지 않음"));
        }
        // 본문은 보내는 길(HTTP 라이브러리)이 가져간다 — 키 구역과 같다.
        let body = token_body(&token);
        drop(token);
        let Some((status, value)) = link.post_raw(MESSENGER_TELEGRAM_PATH, body).await else {
            return Token::Done(Outcome::failed(super::BROKEN));
        };
        if status == 200 {
            let bot = value["bot"].as_str().unwrap_or_default().to_string();
            return Token::Registered(bot);
        }
        if let Some(message) = demo_message(&value) {
            prompter.say(&message);
            return Token::Done(Outcome::failed(&message));
        }
        let Some(refusal) = refusal_of(&value) else {
            return Token::Done(Outcome::failed(super::BROKEN));
        };
        prompter.say(&refusal.to_string());
        if gives_up(refusal) {
            return Token::Done(Outcome::failed(&refusal.to_string()));
        }
        prompter.say("다시 넣으세요");
    }
}

/// 시연 모드 거절이면 서버의 글.
fn demo_message(value: &Value) -> Option<String> {
    (value["error"] == "demo_mode")
        .then(|| value["message"].as_str().unwrap_or_default().to_string())
}

/// 코드 단계: 틀리면 다시 묻는다. 잠김 · 이미 연결된 사람이면 실패.
async fn pair(link: &Link, prompter: &mut Prompter<'_>) -> Outcome {
    loop {
        let Some(code) = prompter.line(SETUP_MESSENGER_CODE_PROMPT) else {
            return Outcome::ended();
        };
        let code = code.trim().to_uppercase();
        if code.is_empty() {
            return Outcome::skipped("코드를 넣지 않음(토큰은 등록됨)");
        }
        let body = serde_json::json!({ "code": code }).to_string().into_bytes();
        let Some((status, value)) = link.post_raw(MESSENGER_PAIRING_APPROVE_PATH, body).await
        else {
            return Outcome::failed(super::BROKEN);
        };
        match (status, value["error"].as_str()) {
            (200, _) => {
                prompter.say(SETUP_MESSENGER_PAIRED);
                return Outcome::done("토큰 등록 · 연결");
            }
            (_, Some("wrong_code")) => prompter.say(MESSENGER_WRONG_CODE),
            (_, Some("locked")) => {
                prompter.say(MESSENGER_LOCKED);
                return Outcome::failed(MESSENGER_LOCKED);
            }
            (_, Some("owner_exists")) => {
                prompter.say(MESSENGER_OWNER_EXISTS);
                return Outcome::failed(MESSENGER_OWNER_EXISTS);
            }
            _ => return Outcome::failed(super::BROKEN),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 토큰_본문은_화면과_같은_모양이다() {
        let body = token_body("a\"b");
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value, serde_json::json!({ "token": "a\"b" }));
    }
}
