// 다듬기 E 3번: 담당 탭 줄의 "부르기". 1:1 담당 탭을 고르면 탭 줄 오른쪽 끝에 "○○ 부르기" 단추 하나만 둔다.
// 팀 방에서는 부르기 단추 · 아이콘을 두지 않는다(기능 H 1번 대조 — 예전에는 잠긴 e2e가 팀 방에서 아이콘을 눌러
// 담당 탭마다 작은 아이콘을 붙였다. e2e는 이제 담당 탭을 고른 뒤 "○○ 부르기"를 누른다).
// 가벼운 화면 목록의 부르기 단추(`summonButton`, 글 "부르기")는 그대로다.
// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { SUMMON_BUTTON, SUMMON_BUTTON_CLASS, summonButton } from "../../web/src/office/summon-view";
import { placeSummons, summonLabel } from "../../web/src/office/summon-tab";

const AGENTS = ["scout", "strategy", "risk", "review", "trader", "leader"];
const NAMES: Record<string, string> = {
  scout: "뉴스 공시 시세",
  strategy: "전략",
  risk: "리스크",
  review: "복기",
  trader: "매매",
  leader: "팀장",
};
const nameOf = (id: string): string => NAMES[id] ?? id;

/** 지금 화면처럼 탭 줄을 만든다(팀 방 + 담당 탭). */
function tabRow(): HTMLElement {
  const tabs = document.createElement("div");
  for (const key of ["team", ...AGENTS]) {
    const tab = document.createElement("button");
    tab.setAttribute("role", "tab");
    tab.textContent = key === "team" ? "팀 방" : nameOf(key);
    tabs.append(tab);
  }
  return tabs;
}

function summons(tabs: HTMLElement): HTMLButtonElement[] {
  return [...tabs.querySelectorAll<HTMLButtonElement>(`button.${SUMMON_BUTTON_CLASS}`)];
}

describe("담당 탭 줄의 부르기", () => {
  it("1:1 담당 탭을 고르면 그 담당의 부르기 단추 하나만 탭 줄 오른쪽 끝에 둔다", () => {
    const tabs = tabRow();
    const called: string[] = [];
    placeSummons(tabs, ["team", ...AGENTS], "risk", nameOf, (agent) => called.push(agent));
    const buttons = summons(tabs);
    expect(buttons).toHaveLength(1);
    expect(buttons[0].dataset.agent).toBe("risk");
    expect(buttons[0].textContent).toBe(summonLabel("리스크"));
    expect(summonLabel("리스크")).toBe("리스크 부르기");
    expect(tabs.lastElementChild, "탭 줄 오른쪽 끝").toBe(buttons[0]);
    buttons[0].click();
    expect(called).toEqual(["risk"]);
  });

  it("팀 방에서는 글 단추 없이 담당 탭마다 작은 아이콘 단추만 탭 바로 뒤에 붙인다", () => {
    // 대조(기능 H 1번): 팀 방 탭 줄에는 부르기 단추 · 아이콘을 아예 그리지 않는다(e2e가 담당 탭을 먼저 고른다).
    const tabs = tabRow();
    placeSummons(tabs, ["team", ...AGENTS], "team", nameOf, () => undefined);
    expect(summons(tabs)).toHaveLength(0);
    expect(tabs.querySelectorAll(".summon-icon")).toHaveLength(0);
  });

  it("다시 그리면 앞서 붙인 부르기 단추를 남기지 않는다", () => {
    const tabs = tabRow();
    placeSummons(tabs, ["team", ...AGENTS], "team", nameOf, () => undefined);
    placeSummons(tabs, ["team", ...AGENTS], "leader", nameOf, () => undefined);
    expect(summons(tabs).map((b) => b.dataset.agent)).toEqual(["leader"]);
  });

  it("가벼운 화면 목록의 부르기 단추는 그대로 글 '부르기'다", () => {
    const button = summonButton("risk", () => undefined);
    expect(button.textContent).toBe(SUMMON_BUTTON);
    expect(button.classList.contains("summon-icon")).toBe(false);
  });

  it("사무실 화면은 탭 사이사이에 글 부르기 단추를 끼우지 않고 placeSummons로 단다", () => {
    const sources = import.meta.glob<string>("../../web/src/office/main.ts", { query: "?raw", import: "default", eager: true });
    const main = sources["../../web/src/office/main.ts"];
    expect(main).toContain("placeSummons(");
    expect(main).not.toMatch(/tabs\.append\(summonButton\(/);
  });
});
