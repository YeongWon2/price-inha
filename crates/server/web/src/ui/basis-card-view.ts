// 근거 카드 그리기(server 50차 · C23 · V68 · V69): basis-card.ts가 만든 글을 DOM으로. 글은 모두 글자(textContent)로 넣는다.
// 히스토리 화면의 매매 상세와 사무실의 근거 패널이 같은 카드를 쓴다. 모양은 basis-card.css(토큰만).
//
// 카드 모양(시험이 고르는 표시):
// - 뿌리 `section.basis-card[data-role="basis-card"]`, 제목 "근거 카드".
// - 기록이 없으면(옛 매매) `[data-role="no-record"]` "기록 없음" 한 줄과 자료 줄만.
// - 찬반 수 `[data-role="votes"]`, 담당 줄 목록 `ol[data-role="opinions"]`의 `li[data-agent=id][data-tone=…]`
//   (기권이면 `data-abstained="true"`): 이름 `[data-part="name"]` · 방향 `[data-part="direction"]` ·
//   확신도 막대 `[data-part="confidence"]`(role="meter", 기권이면 없음) · 이유 `[data-part="reason"]` ·
//   보는 기간 `[data-part="horizon"]`.
// - 팀장 칸 `[data-role="leader"]`: 결정 `[data-part="decision"]` · 요약 `[data-part="summary"]` ·
//   뒤집힐 조건 `[data-part="flip"]`. 자료 이름 `[data-role="sources"]`.
import type { BasisCard } from "./basis-card";

/** 근거 카드 한 장. */
export function renderBasisCard(card: BasisCard): HTMLElement {
  void card;
  return document.createElement("section");
}
