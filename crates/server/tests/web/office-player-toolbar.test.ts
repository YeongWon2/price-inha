// 도구 줄 · 조작 안내 · "누구와 대화?" 목록 · 말 걸기 안내 · 내 이름표(server 32차 3번 · 6번 · 7번, V56)의 DOM.
// 도구 줄 8개(전체 보기 · 왼쪽 돌리기 · 오른쪽 돌리기 · 입체 · 위에서 · 따라가기 · + · −), 오른쪽 아래 조작 안내
// "방향키 이동 · / 대화 · 끌기 화면 이동", 내 이름표는 원본 녹색(#4e745b) 바탕에 흰 글자(deskrpg.md 11절).
import { beforeEach, describe, expect, it } from "vitest";
import { MY_TAG_BACKGROUND, MY_TAG_CLASS, makeNameTag } from "../../web/src/office/player-tag";
import {
  CONTROLS_HINT,
  TOOLS,
  mountControlsHint,
  mountToolbar,
  setHint,
  showChooser,
  type ToolId,
} from "../../web/src/office/toolbar";

const css = Object.values(
  import.meta.glob<string>("../../web/src/office/office.css", { query: "?raw", import: "default", eager: true }),
)[0];

let root: HTMLElement;
beforeEach(() => {
  document.body.replaceChildren();
  root = document.createElement("div");
  document.body.appendChild(root);
});

describe("도구 줄", () => {
  it("단추 8개가 정한 순서와 글로 있다", () => {
    // 검증: V56
    expect(TOOLS.map((t) => t.label)).toEqual(["전체 보기", "왼쪽 돌리기", "오른쪽 돌리기", "입체", "위에서", "따라가기", "+", "−"]);
    const bar = mountToolbar(root, () => {});
    expect(root.contains(bar.element)).toBe(true);
    expect(bar.element.getAttribute("role")).toBe("toolbar");
    const buttons = [...bar.element.querySelectorAll("button")];
    expect(buttons).toHaveLength(8);
    expect(buttons.map((b) => b.textContent)).toEqual(TOOLS.map((t) => t.label));
    expect(buttons.map((b) => b.dataset.tool)).toEqual(TOOLS.map((t) => t.id));
    for (const b of buttons) expect(b.getAttribute("type")).toBe("button");
  });

  it("단추를 누르면 그 도구를 알린다", () => {
    // 검증: V56
    const seen: ToolId[] = [];
    const bar = mountToolbar(root, (id) => seen.push(id));
    for (const b of bar.element.querySelectorAll<HTMLButtonElement>("button")) b.click();
    expect(seen).toEqual(TOOLS.map((t) => t.id));
  });

  it("따라가기 단추는 켜짐을 aria-pressed로 보인다", () => {
    // 검증: V56
    const bar = mountToolbar(root, () => {});
    const follow = bar.element.querySelector<HTMLButtonElement>('button[data-tool="follow"]');
    expect(follow?.getAttribute("aria-pressed")).toBe("true");
    bar.setFollowing(false);
    expect(follow?.getAttribute("aria-pressed")).toBe("false");
    bar.setFollowing(true);
    expect(follow?.getAttribute("aria-pressed")).toBe("true");
    const off = mountToolbar(document.createElement("div"), () => {}, false);
    expect(off.element.querySelector('button[data-tool="follow"]')?.getAttribute("aria-pressed")).toBe("false");
  });
});

describe("조작 안내", () => {
  it("오른쪽 아래 안내 글은 원본 문장 그대로다", () => {
    // 검증: V56
    expect(CONTROLS_HINT).toBe("방향키 이동 · / 대화 · 끌기 화면 이동");
    const hint = mountControlsHint(root);
    expect(root.contains(hint)).toBe(true);
    expect(hint.id).toBe("controls-hint");
    expect(hint.textContent).toBe(CONTROLS_HINT);
  });

  it("말 걸기 안내는 글이 있으면 보이고 없으면 숨는다", () => {
    // 검증: V56
    const el = document.createElement("div");
    setHint(el, "전략을 클릭하거나 /를 눌러 대화");
    expect(el.hidden).toBe(false);
    expect(el.textContent).toBe("전략을 클릭하거나 /를 눌러 대화");
    setHint(el, null);
    expect(el.hidden).toBe(true);
  });
});

describe("누구와 대화? 목록", () => {
  it("제목과 가까운 순 단추를 그리고, 고르면 그 담당을 알린다", () => {
    // 검증: V56 E42
    const picked: string[] = [];
    const list = showChooser(
      root,
      [
        { id: "risk", name: "리스크" },
        { id: "news", name: "뉴스 공시 시세" },
      ],
      (id) => picked.push(id),
    );
    expect(root.contains(list)).toBe(true);
    expect(list.textContent).toContain("누구와 대화?");
    const buttons = [...list.querySelectorAll<HTMLButtonElement>("button[data-agent]")];
    expect(buttons.map((b) => b.dataset.agent)).toEqual(["risk", "news"]);
    expect(buttons.map((b) => b.textContent)).toEqual(["리스크", "뉴스 공시 시세"]);
    buttons[1].click();
    expect(picked).toEqual(["news"]);
  });

  it("다시 그리면 있던 목록을 바꾼다(둘이 되지 않는다)", () => {
    // 검증: V56 E42
    showChooser(root, [{ id: "risk", name: "리스크" }, { id: "news", name: "뉴스" }], () => {});
    showChooser(root, [{ id: "leader", name: "팀장" }, { id: "trader", name: "트레이더" }], () => {});
    const buttons = [...root.querySelectorAll<HTMLButtonElement>("button[data-agent]")];
    expect(buttons.map((b) => b.dataset.agent)).toEqual(["leader", "trader"]);
  });
});

describe("내 이름표", () => {
  it("담당과 같은 이름표에 내 것 표시가 더해지고 글은 이름뿐이다", () => {
    // 검증: V56
    const mine = makeNameTag(document, "나", true);
    expect(mine.classList.contains("tag")).toBe(true);
    expect(mine.classList.contains(MY_TAG_CLASS)).toBe(true);
    expect(mine.textContent).toBe("나");
    expect(mine.title).toBe("나");
    const other = makeNameTag(document, "전략", false);
    expect(other.classList.contains("tag")).toBe(true);
    expect(other.classList.contains(MY_TAG_CLASS)).toBe(false);
  });

  it("내 이름표는 원본 녹색 바탕에 흰 글자다", () => {
    // 검증: V56
    expect(MY_TAG_BACKGROUND).toBe("#4e745b");
    const rule = css.match(/\.tag\.me\s*\{([^}]*)\}/);
    expect(rule, ".tag.me 규칙").not.toBeNull();
    const body = (rule as RegExpMatchArray)[1].toLowerCase();
    expect(body).toMatch(/background(-color)?\s*:\s*#4e745b/);
    expect(body).toMatch(/(^|[^-])color\s*:\s*(#fff\b|#ffffff\b|white\b)/);
  });
});
