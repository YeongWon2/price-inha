// 다듬기 E 9번: 사무실 "담당 외형" 고르기에 "나" 줄을 더해 내 이름과 외형을 바꾼다(API는 32차 `POST /api/office/me`
// `{"name"?, "look"?}` → `GET`과 같은 모양). "나"를 고르면 이름 칸 · 이름 바꾸기 단추와 외형 목록(지금 외형이 눌림)이
// 보이고, 바꾸면 서버 답으로 화면(3D)에 알린다. 실패하면 안내만 하고 던지지 않는다. `me`를 주지 않으면 지금 그대로다.
// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";
import { createLookPicker, lookName } from "../../web/src/office/look-picker";
import { ME_TAB_LABEL, createMePick, type MeLook, type MePick } from "../../web/src/office/me-pick";
import { LOOKS } from "../../web/src/office/looks";

const AGENTS = ["scout", "strategy"];
const LOOK_A = LOOKS[0].id;
const LOOK_B = LOOKS[1].id;

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 가짜 "나": 저장 요청을 적고 정해 둔 답을 낸다. */
function fakeMe(start: MeLook, fail = false): { pick: MePick; saved: Partial<MeLook>[]; applied: MeLook[] } {
  let now = { ...start };
  const saved: Partial<MeLook>[] = [];
  const applied: MeLook[] = [];
  const pick: MePick = {
    current: () => now,
    save: (change) => {
      saved.push(change);
      if (fail) return Promise.reject(new Error("400"));
      now = { ...now, ...change };
      return Promise.resolve(now);
    },
    applied: (me) => applied.push(me),
  };
  return { pick, saved, applied };
}

function open(me?: MePick): HTMLElement {
  const panel = document.createElement("div");
  panel.hidden = true;
  document.body.append(panel);
  const picker = createLookPicker({
    panel,
    agents: AGENTS,
    nameOf: (id) => id,
    current: () => ({}),
    choose: () => Promise.resolve({}),
    applied: () => undefined,
    me,
  });
  picker.open();
  return panel;
}

function tab(panel: HTMLElement, text: string): HTMLButtonElement {
  const found = [...panel.querySelectorAll<HTMLButtonElement>('.looks-agents [role="tab"]')].find((b) =>
    (b.textContent ?? "").startsWith(text),
  );
  expect(found, text).toBeDefined();
  return found as HTMLButtonElement;
}

function lookButton(panel: HTMLElement, id: string): HTMLButtonElement {
  const name = lookName(id);
  const found = [...panel.querySelectorAll<HTMLButtonElement>("button.look")].find((b) =>
    (b.querySelector("strong")?.textContent ?? "") === name,
  );
  expect(found, id).toBeDefined();
  return found as HTMLButtonElement;
}

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

describe("외형 고르기의 나 줄", () => {
  it("나를 주면 맨 앞에 나 탭이 있고 고르면 이름 칸과 지금 외형이 보인다", () => {
    const { pick } = fakeMe({ name: "홍길동", look: LOOK_A });
    const panel = open(pick);
    const tabs = [...panel.querySelectorAll('.looks-agents [role="tab"]')];
    expect(tabs[0]?.textContent).toBe(`${ME_TAB_LABEL} · 홍길동`);
    tab(panel, ME_TAB_LABEL).click();
    const input = panel.querySelector<HTMLInputElement>("input#me-name");
    expect(input?.value).toBe("홍길동");
    expect(panel.querySelector('button[data-action="me-name"]')).not.toBeNull();
    expect(lookButton(panel, LOOK_A).getAttribute("aria-pressed")).toBe("true");
    expect(lookButton(panel, LOOK_B).getAttribute("aria-pressed")).toBe("false");
  });

  it("나를 고른 채 외형을 누르면 내 외형을 바꾸고 화면에 알린다", async () => {
    const me = fakeMe({ name: "홍길동", look: LOOK_A });
    const panel = open(me.pick);
    tab(panel, ME_TAB_LABEL).click();
    lookButton(panel, LOOK_B).click();
    await settle();
    expect(me.saved).toEqual([{ look: LOOK_B }]);
    expect(me.applied).toEqual([{ name: "홍길동", look: LOOK_B }]);
    expect(panel.querySelector(".looks-note")?.textContent).toContain(lookName(LOOK_B));
    expect(lookButton(panel, LOOK_B).getAttribute("aria-pressed")).toBe("true");
  });

  it("이름을 바꾸면 다듬은 이름을 보내고 나 탭 글이 바뀐다", async () => {
    const me = fakeMe({ name: "홍길동", look: LOOK_A });
    const panel = open(me.pick);
    tab(panel, ME_TAB_LABEL).click();
    const input = panel.querySelector<HTMLInputElement>("input#me-name") as HTMLInputElement;
    input.value = "  김투자  ";
    panel.querySelector<HTMLButtonElement>('button[data-action="me-name"]')?.click();
    await settle();
    expect(me.saved).toEqual([{ name: "김투자" }]);
    expect(me.applied).toEqual([{ name: "김투자", look: LOOK_A }]);
    expect(tab(panel, ME_TAB_LABEL).textContent).toBe(`${ME_TAB_LABEL} · 김투자`);
  });

  it("빈 이름은 보내지 않고, 서버가 거절하면 안내만 한다", async () => {
    const me = fakeMe({ name: "홍길동", look: LOOK_A }, true);
    const panel = open(me.pick);
    tab(panel, ME_TAB_LABEL).click();
    const input = panel.querySelector<HTMLInputElement>("input#me-name") as HTMLInputElement;
    input.value = "   ";
    panel.querySelector<HTMLButtonElement>('button[data-action="me-name"]')?.click();
    await settle();
    expect(me.saved).toEqual([]);
    lookButton(panel, LOOK_B).click();
    await settle();
    expect(me.saved).toEqual([{ look: LOOK_B }]);
    expect(me.applied).toEqual([]);
    expect(panel.querySelector(".looks-note")?.textContent).toContain("바꾸지 못했습니다");
  });

  it("나를 주지 않으면 담당 탭만 있다(지금 그대로)", () => {
    const panel = open();
    const tabs = [...panel.querySelectorAll('.looks-agents [role="tab"]')].map((b) => b.textContent ?? "");
    expect(tabs).toHaveLength(AGENTS.length);
    expect(tabs.some((t) => t.startsWith(ME_TAB_LABEL))).toBe(false);
  });
});

describe("나 바꾸기 요청", () => {
  it("POST /api/office/me로 바꿀 칸만 보내고 답의 이름 · 외형을 돌려준다", async () => {
    const calls: { url: string; init?: RequestInit }[] = [];
    const fetcher = vi.fn((url: string, init?: RequestInit) => {
      calls.push({ url, init });
      return Promise.resolve(
        new Response(JSON.stringify({ name: "김투자", look: LOOK_B, x: 3, y: 4, dir: "down" }), { status: 200 }),
      );
    });
    let kept: MeLook | null = null;
    const pick = createMePick(fetcher, () => ({ name: "홍길동", look: LOOK_A }), (me) => { kept = me; });
    const result = await pick.save({ look: LOOK_B });
    expect(result).toEqual({ name: "김투자", look: LOOK_B });
    expect(calls[0]?.url).toBe(API.officeMe);
    expect(calls[0]?.init?.method).toBe("POST");
    expect(calls[0]?.init?.credentials).toBe("same-origin");
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({ look: LOOK_B });
    pick.applied(result);
    expect(kept).toEqual({ name: "김투자", look: LOOK_B });
    expect(pick.current()).toEqual({ name: "홍길동", look: LOOK_A });
  });

  it("서버가 거절하거나 모양이 틀리면 던진다", async () => {
    const refused = createMePick(() => Promise.resolve(new Response("{}", { status: 400 })), () => null, () => undefined);
    await expect(refused.save({ name: "x" })).rejects.toThrow();
    const odd = createMePick(() => Promise.resolve(new Response("{\"name\":1}", { status: 200 })), () => null, () => undefined);
    await expect(odd.save({ name: "x" })).rejects.toThrow();
  });

  it("사무실 화면은 외형 고르기에 나를 잇는다", () => {
    const sources = import.meta.glob<string>("../../web/src/office/main.ts", { query: "?raw", import: "default", eager: true });
    expect(sources["../../web/src/office/main.ts"]).toContain("createMePick(");
  });
});
