// 공통 메뉴 줄(server 26차 3번 · R18-1 · V75): 여섯 항목(사무실 · 계좌 · 템플릿 · 규칙 · 히스토리 · 설정)이 그 차례로,
// 주소는 화면 주소 표(`SCREEN_ROOTS`) 그대로, 지금 화면에만 aria-current="page", `nav`에 접근 이름이 있다.
import { describe, expect, it } from "vitest";
import { SCREEN_ROOTS, type ScreenName } from "../../web/src/paths";
import { MENU_ITEMS, menu, mountMenu } from "../../web/src/ui/menu";

const ORDER = [
  ["office", "사무실", "/office/"],
  ["account", "계좌", "/account/"],
  ["templates", "템플릿", "/templates/"],
  ["rules", "규칙", "/rules/"],
  ["history", "히스토리", "/history/"],
  ["setup", "설정", "/"],
] as const;

const ALL: readonly ScreenName[] = ["setup", "keys", "office", "phone", "account", "templates", "rules", "history"];

const links = (nav: HTMLElement) => [...nav.querySelectorAll<HTMLAnchorElement>("a[href]")];

describe("화면 주소 표", () => {
  it("관리 화면 넷의 주소가 표에 있고 메뉴 항목의 주소와 같다", () => {
    // 검증: V75 F12
    expect(SCREEN_ROOTS.account).toBe("/account/");
    expect(SCREEN_ROOTS.templates).toBe("/templates/");
    expect(SCREEN_ROOTS.rules).toBe("/rules/");
    expect(SCREEN_ROOTS.history).toBe("/history/");
    expect(MENU_ITEMS.map((item) => [item.screen, item.label, item.href])).toEqual(ORDER.map((row) => [...row]));
    for (const item of MENU_ITEMS) expect(item.href).toBe(SCREEN_ROOTS[item.screen]);
  });
});

describe("메뉴 줄", () => {
  it("nav 하나에 여섯 항목이 차례대로 있고 누르면 그 화면 주소로 간다", () => {
    // 검증: V75 F12
    const nav = menu("account");
    expect(nav.tagName).toBe("NAV");
    const found = links(nav);
    expect(found.map((a) => a.textContent?.trim())).toEqual(ORDER.map(([, label]) => label));
    expect(found.map((a) => a.getAttribute("href"))).toEqual(ORDER.map(([, , href]) => href));
    // 바깥 주소 · 새 창이 아니다.
    for (const a of found) {
      expect(a.getAttribute("target")).toBeNull();
      expect(a.getAttribute("href")?.startsWith("/")).toBe(true);
    }
  });

  it("nav에 접근 이름이 있고 화면마다 같다", () => {
    // 검증: V75
    const names = ALL.map((screen) => menu(screen).getAttribute("aria-label") ?? "");
    expect(names[0].trim().length).toBeGreaterThan(0);
    expect(new Set(names).size).toBe(1);
  });

  it("지금 화면의 항목에만 aria-current=page가 붙는다", () => {
    // 검증: V75 F12
    for (const [screen, label, href] of ORDER) {
      const current = links(menu(screen)).filter((a) => a.hasAttribute("aria-current"));
      expect(current.length, screen).toBe(1);
      expect(current[0].getAttribute("aria-current")).toBe("page");
      expect(current[0].textContent?.trim()).toBe(label);
      expect(current[0].getAttribute("href")).toBe(href);
    }
  });

  it("키 · 폰 화면에도 같은 메뉴가 달리고 aria-current가 붙는 항목은 없다", () => {
    // 검증: V75
    for (const screen of ["keys", "phone"] as const) {
      const nav = menu(screen);
      expect(links(nav).map((a) => a.textContent?.trim())).toEqual(ORDER.map(([, label]) => label));
      expect(nav.querySelectorAll("[aria-current]").length, screen).toBe(0);
    }
  });

  it("메뉴 자리에 넣으면 자리 안에 nav가 하나 들어가고, 자리가 없으면 null이다", () => {
    // 검증: V75
    document.body.innerHTML = '<div id="menu" data-screen="rules"></div><main><h1>규칙</h1></main>';
    const slot = document.getElementById("menu");
    const nav = mountMenu(slot, "rules");
    expect(nav).not.toBeNull();
    expect(slot?.querySelectorAll("nav").length).toBe(1);
    expect(slot?.contains(nav)).toBe(true);
    expect(slot?.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe("규칙");
    // 두 번 달아도 메뉴는 하나다.
    mountMenu(slot, "rules");
    expect(document.querySelectorAll("nav").length).toBe(1);
    expect(mountMenu(null, "rules")).toBeNull();
  });
});
