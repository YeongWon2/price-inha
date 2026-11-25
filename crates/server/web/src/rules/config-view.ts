// 규칙 화면의 금지 종목 · 업종표 묶음 그리기(server 35차 3번). 26차 공통 부품 · 토큰만 쓴다. 서버를 부르지 않는다
// (screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시) — 묶음 하나 `[data-role="rules-config"]`(카드 제목 "금지 종목 · 업종표") 안에:
// - 금지 종목 표 `[data-role="banned"]`: 줄 `tr[data-banned="<시장>:<종목>"]`(글에 시장 글 "국내" · "미국"과 종목).
//   비었으면 표 대신(또는 표 안에) `[data-role="no-banned"]`.
// - 업종표 `[data-role="sectors"]`: 줄 `tr[data-sector="<종목>"]`(글에 종목과 업종 이름). 비었으면 `[data-role="no-sectors"]`.
// - 업종 없는 종목 경고 `[data-role="unsectored"]`(글은 `unsectoredLine`): `unsectored`가 비면 없다.
// - 고칠 수 있으면(`editable`):
//   - 줄마다 지우기 단추 `button[data-action="remove-banned"]` · `button[data-action="remove-sector"]`.
//   - 금지 종목 더하기: `select[name="banned-market"]`(값 `kr` · `us`) · `input[name="banned-symbol"]` ·
//     `button[data-action="add-banned"]`. 업종표 더하기: `input[name="sector-symbol"]` · `input[name="sector-name"]` ·
//     `button[data-action="add-sector"]`. 더하지 못하면 까닭 글 `[data-role="config-problem"]`(요청은 나가지 않는다).
//   - 저장 단추 `button[data-action="save-config"]`. 저장하는 동안 막힌다.
// - 고칠 수 없으면(원격): 지우기 · 더하기 · 저장 단추와 입력이 없고 `[data-role="config-readonly"]`(글에
//   "이 컴퓨터에서만 바꿀 수 있습니다").
// - 저장 결과: 저장했으면 `[data-role="config-result"]`(글에 "저장했습니다"), 실패면 `[data-role="config-error"]`
//   (`role="alert"`, 글은 `saveMessage`) — 실패해도 고치던 줄은 그대로 남는다.
// - 이 묶음만 읽지 못하면 묶음 안에 `[data-role="config-load-failed"]`(글에 "불러오지 못"; `role="alert"`는 달지 않는다 —
//   다른 묶음(지금 규칙 · 절대 상한 · 개선안)은 그대로 그린다).
import type { ConfigDraft, RulesConfigReply, SaveMessage } from "./config-model";

/** 묶음이 사용자 손짓을 알리는 곳(screen.ts가 단다). 차례는 초안의 줄 차례다. */
export interface ConfigHandlers {
  addBanned(market: string, symbol: string): void;
  removeBanned(index: number): void;
  addSector(symbol: string, sector: string): void;
  removeSector(index: number): void;
  save(): void;
}

/** 묶음의 지금 모습: 읽은 값(경고 · 고칠 수 있는가) · 초안 · 까닭 글 · 저장 결과 · 저장 중인가. */
export interface ConfigState {
  reply: RulesConfigReply;
  draft: ConfigDraft;
  problem: string | null;
  saved: SaveMessage | null;
  saving: boolean;
}

/** 금지 종목 · 업종표 묶음(카드 하나)을 그린다. */
export function renderConfig(state: ConfigState, handlers: ConfigHandlers): HTMLElement {
  void state;
  void handlers;
  throw new Error("todo");
}

/** 이 묶음만 읽지 못했을 때의 묶음(카드 하나, `[data-role="config-load-failed"]`). */
export function renderConfigFailed(): HTMLElement {
  throw new Error("todo");
}
