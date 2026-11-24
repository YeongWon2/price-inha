// 규칙 화면의 그리기(server 28차 2번): 규칙 표 · 절대 상한 · 개선안 목록. 26차 공통 부품 · 토큰만 쓴다.
// 서버를 부르지 않는다(screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시):
// - 규칙 표 `[data-role="rules"]`: 줄 `tr[data-key=키]`, 그 안에 지금 값 `[data-role="value"]` · 상한 `[data-role="cap"]`.
//   최근에 바뀐 칸은 줄에 `data-changed="true"`와 `[data-role="changed"]`(글에 "바뀜"과 앞 값),
//   냉각중 변경이 걸린 칸은 `[data-role="cooling"]`(글에 "다음 거래일부터"와 뒤 값).
// - 절대 상한 `[data-role="caps"]`: 줄 `[data-key=키]`, 글에 이름과 상한.
// - 개선안 목록 `[data-role="proposals"]`: 줄 `[data-proposal=번호]`(`data-state`=상태 글자), 그 안에
//   상태 `[data-role="state"]` · 실수 종류 `[data-role="mistake"]` · 위험 `[data-role="risk"]`(`data-risky`=true|false) ·
//   바뀐 칸 `[data-role="change"][data-key=키]`(글에 앞 값 "→" 뒤 값, 넓힘은 `data-widened="true"`) ·
//   답 결과 `[data-role="answer-result"]`(`data-tone`=문구 종류). 승인 · 거절 단추
//   `button[data-action="approve"]` · `button[data-action="reject"]`는 답할 수 있는 줄(`canAnswer`)에만 있다.
import type { AnswerMessage, ProposalRow, RuleRow } from "./model";

/** 규칙 표. */
export function renderRules(rows: readonly RuleRow[]): HTMLElement {
  void rows;
  throw new Error("todo");
}

/** 원칙의 절대 상한 목록. */
export function renderCaps(rows: readonly { key: string; label: string; cap: string }[]): HTMLElement {
  void rows;
  throw new Error("todo");
}

/** 개선안 목록. `notes`는 개선안 번호 → 그 줄에 보일 답 결과 문구. */
export function renderProposals(rows: readonly ProposalRow[], notes: ReadonlyMap<number, AnswerMessage>): HTMLElement {
  void rows;
  void notes;
  throw new Error("todo");
}
