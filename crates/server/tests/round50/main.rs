//! 50차(additions.md 차수 표 19번 — 근거 카드 화면 + 승인 카드 보강)의 Rust 쪽 시험.
//!
//! - `scaffold`: 발판이 지금 구현에서 서는지 — 서버가 뜨고, 판단된 매매(의견이 있는 것 · 옛 모양)와 금액 한도를
//!   넘는 주문의 승인 요청을 만들 수 있다.
//! - `basis`: C23 · E53 · V68 · V69 — `GET /api/trades/{id}/basis`에 담당별 의견 · 팀장 요약 · 뒤집힐 조건 ·
//!   찬반 수 · 자료 이름이 더해지고(옛 칸 그대로), 의견이 없는 옛 매매는 빈 목록 · 팀장 요약 없음이다.
//!   히스토리의 매매 하나(`GET /api/trades/{id}`)의 근거에도 같은 칸이 실린다.
//! - `approval_card`: C24 · R4-4 · V55 — 주문 승인 카드(`GET /api/approvals/{id}` · `office.approval`)에 투표 수와
//!   한도 적용 전 → 후가 실리고, 그 값은 요청의 summary에 든 것과 같으며 지문은 그대로다.
//!
//! 웹 승인이 `approved`가 되길 기대하는 시험은 없다(시연이 아니면 P12 재점검으로 막힌다). 화면 동작의 시험은
//! vitest(`tests/web/basis-card*.test.ts` · `approval-extras*.test.ts`)에 있다.

mod approval_card;
mod basis;
mod common;
mod scaffold;
