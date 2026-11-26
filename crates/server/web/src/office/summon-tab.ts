// 담당 탭 줄의 부르기 단추(다듬기 E 3번, server 33차 7번 다듬기). 탭 사이사이에 글 단추 여섯이 끼어 두 줄로 붐비던 것을
// 줄인다:
// - 1:1 담당 탭을 골랐으면 탭 줄 오른쪽 끝에 "○○ 부르기" 단추 하나(그 담당).
// - 팀 방이면 글 단추 없이 담당 탭마다 작은 아이콘 단추(`.summon-icon`, 이름은 `aria-label` · `title`)만 탭 바로 뒤에.
// 단추는 모두 `summon-view.ts`의 부르기 단추(`button.summon[data-agent]`)를 바탕으로 만든다. 가벼운 화면 목록의
// 부르기 단추는 그대로다. 글은 `textContent`로만 넣는다.

import { SUMMON_BUTTON_CLASS, summonButton } from "./summon-view";

/** 팀 방 담당 탭 뒤의 작은 부르기 아이콘 글. */
export const SUMMON_ICON = "📣";
/** 아이콘 단추의 class(부르기 단추 class에 더한다). */
const ICON_CLASS = "summon-icon";
/** 팀 방 대화 키. */
const ROOM_TEAM = "team";

/** "○○ 부르기". */
export function summonLabel(name: string): string {
  return `${name} 부르기`;
}

/**
 * `tabs`(탭 줄: `keys` 순서의 탭 단추들)에 부르기 단추를 단다. 앞서 단 부르기 단추는 먼저 뗀다.
 * `conversation`이 담당이면 끝에 그 담당 단추 하나, 팀 방이면 담당 탭마다 아이콘 단추.
 */
export function placeSummons(
  tabs: HTMLElement,
  keys: readonly string[],
  conversation: string,
  nameOf: (id: string) => string,
  onSummon: (agent: string) => void,
): void {
  for (const old of tabs.querySelectorAll(`button.${SUMMON_BUTTON_CLASS}`)) old.remove();
  if (conversation !== ROOM_TEAM) {
    const button = summonButton(conversation, onSummon);
    button.textContent = summonLabel(nameOf(conversation));
    tabs.append(button);
    return;
  }
  const tabList = [...tabs.querySelectorAll<HTMLElement>('[role="tab"]')];
  keys.forEach((key, index) => {
    const tab = tabList[index];
    if (key === ROOM_TEAM || !tab) return;
    const button = summonButton(key, onSummon);
    const label = summonLabel(nameOf(key));
    button.classList.add(ICON_CLASS);
    button.textContent = SUMMON_ICON;
    button.title = label;
    button.setAttribute("aria-label", label);
    tab.after(button);
  });
}
