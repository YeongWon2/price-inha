//! 7번: `DELETE /api/messenger/telegram`이 성공하면 답에 `notice` 칸 하나를 더한다 — 텔레그램 쪽 토큰은 살아 있으니
//! BotFather에서 폐기하라는 안내([`MESSENGER_REVOKE_HINT`]). 옛 칸(`removed`)은 그대로다. 텔레그램은 부르지 않는다.

use price_inha::{MESSENGER_REVOKE_HINT, MESSENGER_TELEGRAM_PATH};
use serde_json::json;

use crate::common::{Running, keys_of, send_json};

// 검증: V88
#[test]
fn 토큰_지우기_안내는_텔레그램_쪽_토큰과_botfather를_말한다() {
    assert!(MESSENGER_REVOKE_HINT.contains("텔레그램 쪽 토큰은 살아 있습니다"));
    assert!(MESSENGER_REVOKE_HINT.contains("BotFather"));
    assert!(MESSENGER_REVOKE_HINT.contains("폐기"));
}

// 검증: V88 V87
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 토큰을_지우면_답에_botfather_폐기_안내가_붙고_옛_칸은_그대로다() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), false).await;
    let reply = send_json(
        running.addr(),
        reqwest::Method::DELETE,
        MESSENGER_TELEGRAM_PATH,
        None,
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["removed"], json!(true), "{}", reply.text);
    assert_eq!(
        reply.body["notice"],
        json!(MESSENGER_REVOKE_HINT),
        "{}",
        reply.text
    );
    assert_eq!(keys_of(&reply.body), ["notice", "removed"], "칸 하나만 더한다");
    running.assert_stayed_home().await;
    running.close().await;
}
