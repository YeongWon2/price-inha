// 공통 메뉴 줄(server 26차 3번 · R18-1 · V75): 모든 화면 위에 같은 메뉴 — 사무실 · 계좌 · 템플릿 · 규칙 · 히스토리 · 설정.
// 순수 DOM 함수(프레임워크 없음). 주소는 `SCREEN_ROOTS`의 값을 쓴다.
// 메뉴를 다는 화면은 모두 이 파일을 거치므로 토큰 파일과 메뉴 모양도 여기서 들인다.
import "./tokens.css";
import "./menu.css";
import { SCREEN_ROOTS, type ScreenName } from "../paths";

/** 메뉴에 나오는 화면(차례 그대로). 키 · 폰 화면은 메뉴에 항목이 없지만 메뉴를 단다. */
export type MenuScreen = "office" | "account" | "templates" | "rules" | "history" | "setup";

/** 메뉴 항목 하나: 화면 · 보이는 이름 · 주소. */
export interface MenuItem {
  screen: MenuScreen;
  label: string;
  href: string;
}

/** 항목 차례와 보이는 이름. */
const LABELS: ReadonlyArray<readonly [MenuScreen, string]> = [
  ["office", "사무실"],
  ["account", "계좌"],
  ["templates", "템플릿"],
  ["rules", "규칙"],
  ["history", "히스토리"],
  ["setup", "설정"],
];

/** 메뉴 항목 여섯(사무실 · 계좌 · 템플릿 · 규칙 · 히스토리 · 설정 차례). */
export const MENU_ITEMS: readonly MenuItem[] = LABELS.map(([screen, label]) => ({
  screen,
  label,
  href: SCREEN_ROOTS[screen],
}));

/** 메뉴 줄의 접근 이름(화면마다 같다). */
const MENU_LABEL = "주 메뉴";

/** 메뉴 줄 클래스(모양은 menu.css). */
const MENU_CLASS = "app-menu";

/**
 * 메뉴 줄 `nav`(접근 이름 `aria-label`이 있다)를 만든다. 항목은 링크(`a[href]`)이고,
 * 지금 화면(`current`)의 항목에만 `aria-current="page"`가 붙는다. 키 · 폰처럼 메뉴에 없는 화면이면 붙는 항목이 없다.
 */
export function menu(current: ScreenName): HTMLElement {
  const nav = document.createElement("nav");
  nav.className = MENU_CLASS;
  nav.setAttribute("aria-label", MENU_LABEL);
  for (const item of MENU_ITEMS) {
    const link = document.createElement("a");
    link.href = item.href;
    link.textContent = item.label;
    if (item.screen === current) link.setAttribute("aria-current", "page");
    nav.append(link);
  }
  return nav;
}

/** 화면 HTML의 메뉴 자리(`#menu`)에 메뉴 줄을 넣고 돌려준다. 자리가 없으면 아무것도 하지 않고 `null`. */
export function mountMenu(slot: HTMLElement | null, current: ScreenName): HTMLElement | null {
  if (!slot) return null;
  const nav = menu(current);
  // 두 번 달아도 하나: 자리 안을 새 메뉴로 바꾼다.
  slot.replaceChildren(nav);
  // 좁은 폭에서 가로로 밀린 메뉴라도 지금 화면 항목이 보이게 한다(페이지는 움직이지 않는다).
  const here = nav.querySelector<HTMLElement>('[aria-current="page"]');
  if (here && nav.scrollWidth > nav.clientWidth) {
    nav.scrollLeft = Math.max(0, here.offsetLeft - nav.clientWidth / 2 + here.offsetWidth / 2);
  }
  return nav;
}
