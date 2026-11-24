// 템플릿 화면의 그리기(server 27차 3번): 칸 표 · 승격 기준 표 · 판 이력 · 실전 안내. 26차 공통 부품 · 토큰만 쓴다.
// 서버를 부르지 않는다(screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시):
// - 칸 표 `[data-role="fields"]`: 줄 `tr[data-key=키]`, 그 안에 지금 값 `[data-role="value"]` · 상한 `[data-role="cap"]` ·
//   입력 `input`(잠긴 칸은 `disabled`) · 칸 옆 안내 `[data-role="field-note"]`.
// - 승격 기준 표 `[data-role="promotion"]`: 줄 `tr[data-criterion=기준]`, 모자란 줄은 `data-short="true"`.
// - 판 이력 `[data-role="revisions"]`: 판 `[data-revision=번호]`, 바뀐 칸 `[data-key=키]`, 넓힘은 `data-widened="true"`와 "넓힘" 글.
// - 실전 안내 `[data-role="live-notice"]`(글은 `LIVE_NOTICE`).
import type { FieldRow, PromotionRow, RevisionRow } from "./model";

/** 칸 표. 잠긴 칸의 입력은 막는다. */
export function renderFields(rows: readonly FieldRow[]): HTMLElement {
  void rows;
  throw new Error("todo");
}

/** 칸 옆 안내를 모두 지우고 `notes`의 칸 옆에 글을 단다(상한 넘음 E5). */
export function showFieldNotes(table: HTMLElement, notes: readonly { key: string; text: string }[]): void {
  void table;
  void notes;
  throw new Error("todo");
}

/** 칸 표의 입력에서 바꾼 값만 모은다(빈 칸 · 지금 값과 같은 칸은 뺀다). 키 → 십진수 글자. */
export function changedValues(table: HTMLElement, rows: readonly FieldRow[]): Record<string, string> {
  void table;
  void rows;
  throw new Error("todo");
}

/** 승격 기준 표. 모자란 줄은 `data-short="true"`. */
export function renderPromotion(rows: readonly PromotionRow[]): HTMLElement {
  void rows;
  throw new Error("todo");
}

/** 판 이력. 판마다 바뀐 칸과 넓힘 표시. */
export function renderRevisions(rows: readonly RevisionRow[]): HTMLElement {
  void rows;
  throw new Error("todo");
}

/** 실전 안내 줄(`LIVE_NOTICE`). */
export function liveNotice(): HTMLElement {
  throw new Error("todo");
}
