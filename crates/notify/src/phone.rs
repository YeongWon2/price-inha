//! 폰 화면(PWA) 묶음. `web/phone/`의 파일을 실행 파일에 넣어 서버가 내준다.
//!
//! 약속(시험이 확인한다):
//! - 파일: `index.html` `app.js` `sw.js` `manifest.webmanifest` 와 아이콘(`icon-192.png` `icon-512.png`).
//! - 글자 `three`가 어느 파일에도 없다(3D를 불러오지 않는다).
//! - `index.html`에 승인 카드 틀(`data-card="approval"`), 알림 목록(`id="notice-list"`),
//!   하루 정리(`id="daily-summary"` 한 구역 안에 `data-part="done"` `data-part="pnl"` `data-part="review"`),
//!   "3D 사무실" 단추(`data-action="open-office"`)가 있다.
//! - 승인 카드: `data-action="approve"` · `data-action="reject"` 단추, 6자리 코드 칸
//!   (`inputmode="numeric"` `maxlength="6"` `data-field="code"`), 승인 번호와 내용 지문을 싣는 자리
//!   (`data-approval-id` · `data-fingerprint`).
//! - `app.js`는 `pushManager.subscribe`와 `applicationServerKey`를 쓰고 서비스 워커(`sw.js`)를 등록한다.
//! - `sw.js`는 `push` · `notificationclick` 이벤트를 듣고 `showNotification`을 부른다.
//! - `manifest.webmanifest`는 JSON이고 `name` `id` `scope` `start_url` `display`(standalone) `icons`가 있다.

/// 내줄 파일 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    /// 경로(`/` 없이 시작, 예: `sw.js`).
    pub path: &'static str,
    /// `Content-Type` 값.
    pub content_type: &'static str,
    /// 내용.
    pub body: &'static [u8],
}

/// 폰 묶음의 모든 파일.
pub fn phone_assets() -> &'static [Asset] {
    todo!()
}

/// 경로로 파일 하나를 찾는다. 없으면 `None`.
pub fn phone_asset(path: &str) -> Option<&'static Asset> {
    let _ = path;
    todo!()
}
