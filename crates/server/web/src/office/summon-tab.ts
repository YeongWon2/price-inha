// 담당 탭 줄의 부르기 단추(다듬기 E 3번). 뼈대.

/** 팀 방 담당 탭 뒤의 작은 부르기 아이콘 글. */
export const SUMMON_ICON = "📣";

/** "○○ 부르기". */
export function summonLabel(name: string): string {
  throw new Error(`todo ${name}`);
}

/** 탭 줄에 부르기 단추를 단다. */
export function placeSummons(
  tabs: HTMLElement,
  keys: readonly string[],
  conversation: string,
  nameOf: (id: string) => string,
  onSummon: (agent: string) => void,
): void {
  void tabs; void keys; void conversation; void nameOf; void onSummon;
  throw new Error("todo");
}
