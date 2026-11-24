// 공통 메뉴 줄(server 26차 3번 · R18-1 · V75): 모든 화면 위에 같은 메뉴 — 사무실 · 계좌 · 템플릿 · 규칙 · 히스토리 · 설정.
// 순수 DOM 함수(프레임워크 없음). 주소는 `SCREEN_ROOTS`의 값을 쓴다.
import type { ScreenName } from "../paths";

/** 메뉴에 나오는 화면(차례 그대로). 키 · 폰 화면은 메뉴에 항목이 없지만 메뉴를 단다. */
export type MenuScreen = "office" | "account" | "templates" | "rules" | "history" | "setup";

/** 메뉴 항목 하나: 화면 · 보이는 이름 · 주소. */
export interface MenuItem {
  screen: MenuScreen;
  label: string;
  href: string;
}

/** 메뉴 항목 여섯(사무실 · 계좌 · 템플릿 · 규칙 · 히스토리 · 설정 차례). 뼈대: 구현이 채운다. */
export const MENU_ITEMS: readonly MenuItem[] = [];

/**
 * 메뉴 줄 `nav`(접근 이름 `aria-label`이 있다)를 만든다. 항목은 링크(`a[href]`)이고,
 * 지금 화면(`current`)의 항목에만 `aria-current="page"`가 붙는다. 키 · 폰처럼 메뉴에 없는 화면이면 붙는 항목이 없다.
 */
export function menu(current: ScreenName): HTMLElement {
  void current;
  throw new Error("todo: server 26차 메뉴 줄");
}

/** 화면 HTML의 메뉴 자리(`#menu`)에 메뉴 줄을 넣고 돌려준다. 자리가 없으면 아무것도 하지 않고 `null`. */
export function mountMenu(slot: HTMLElement | null, current: ScreenName): HTMLElement | null {
  void slot;
  void current;
  throw new Error("todo: server 26차 메뉴 달기");
}
