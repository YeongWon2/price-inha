// 담당 탭 줄의 부르기 단추(다듬기 E 3번, server 33차 7번 다듬기, 기능 H 1번). 탭 사이사이에 글 단추 여섯이 끼어
// 두 줄로 붐비던 것을 줄인다:
// - 1:1 담당 탭을 골랐으면 탭 줄 오른쪽 끝에 "○○ 부르기" 단추 하나(그 담당).
// - 팀 방이면 부르기 단추 · 아이콘을 그리지 않는다(부르려면 담당 탭을 고른다).
// 단추는 `summon-view.ts`의 부르기 단추(`button.summon[data-agent]`)를 바탕으로 만든다. 가벼운 화면 목록의
// 부르기 단추는 그대로다. 글은 `textContent`로만 넣는다.

import { SUMMON_BUTTON_CLASS, summonButton } from "./summon-view";

/** 팀 방 대화 키. */
const ROOM_TEAM = "team";

/** "○○ 부르기". */
export function summonLabel(name: string): string {
  return `${name} 부르기`;
}

/**
 * `tabs`(탭 줄)에 부르기 단추를 단다. 앞서 단 부르기 단추는 먼저 뗀다.
 * `conversation`이 담당이면 끝에 그 담당 단추 하나, 팀 방이면 아무것도 달지 않는다.
 * `_keys`(탭 순서)는 부르는 쪽(main.ts)과의 약속으로 받아만 둔다.
 */
export function placeSummons(
  tabs: HTMLElement,
  _keys: readonly string[],
  conversation: string,
  nameOf: (id: string) => string,
  onSummon: (agent: string) => void,
): void {
  for (const old of tabs.querySelectorAll(`button.${SUMMON_BUTTON_CLASS}`)) old.remove();
  if (conversation === ROOM_TEAM) return;
  const button = summonButton(conversation, onSummon);
  button.textContent = summonLabel(nameOf(conversation));
  tabs.append(button);
}
