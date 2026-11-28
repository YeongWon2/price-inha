// (결정 36) 체험판 안내 줄: 설정 상태에 `trial: true`(price-inha demo가 만든 체험 폴더)가 오면, 설정 화면에서 시연에서
// 가짜로 도는 자리(증권 키 · AI 연결 · 공시 · 뉴스 키 · 메신저와 자동 실행) 바로 앞에 "체험판에서는 … · 실제판에서 쓸 수
// 있습니다" 한 줄을 실제와 같은 자리에 보인다. `trial`이 없으면 아무것도 더하지 않는다. 여러 번 불러도 한 줄씩만.
// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { TRIAL_SPOTS, placeTrialNotes } from "../../web/src/setup/trial";

const ANCHORS = ["reg-broker", "reg-ai", "reg-slot-dart", "setup-extras"];

/** 설정 화면의 자리들(index.html의 id)을 만든다. */
function page(): HTMLElement {
  const root = document.createElement("div");
  for (const id of ANCHORS) {
    const box = document.createElement("section");
    box.id = id;
    root.append(box);
  }
  document.body.append(root);
  return root;
}

function notes(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>("[data-role='trial-note']"));
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("설정 화면 · 체험판 안내 줄", () => {
  it("자리마다 체험판 안내가 있고, 모두 실제판을 가리킨다", () => {
    // 검증: V54 E56
    expect(TRIAL_SPOTS.map((spot) => spot.anchor)).toEqual(ANCHORS);
    for (const spot of TRIAL_SPOTS) {
      expect(spot.text.startsWith("체험판에서는")).toBe(true);
      expect(spot.text).toContain("실제판");
    }
    const all = TRIAL_SPOTS.map((spot) => spot.text).join(" ");
    for (const word of ["증권", "AI", "공시", "텔레그램", "자동 실행"]) expect(all).toContain(word);
  });

  it("trial이면 그 자리 바로 앞에 한 줄씩 그린다", () => {
    // 검증: V54 E56
    const root = page();
    expect(placeTrialNotes(document, { trial: true, demo: true })).toBe(ANCHORS.length);
    const drawn = notes(root);
    expect(drawn).toHaveLength(ANCHORS.length);
    for (const spot of TRIAL_SPOTS) {
      const anchor = document.getElementById(spot.anchor);
      const before = anchor?.previousElementSibling as HTMLElement | null;
      expect(before?.dataset.role).toBe("trial-note");
      expect(before?.textContent).toBe(spot.text);
    }
  });

  it("다시 불러도 줄이 늘지 않는다", () => {
    // 검증: V54
    const root = page();
    placeTrialNotes(document, { trial: true });
    expect(placeTrialNotes(document, { trial: true })).toBe(0);
    expect(notes(root)).toHaveLength(ANCHORS.length);
  });

  it("trial이 없으면(실제판 · run --demo) 아무것도 그리지 않는다", () => {
    // 검증: V54
    const root = page();
    expect(placeTrialNotes(document, { demo: true })).toBe(0);
    expect(placeTrialNotes(document, {})).toBe(0);
    expect(notes(root)).toHaveLength(0);
  });

  it("자리가 아직 없으면 건너뛴다", () => {
    // 검증: V54
    expect(placeTrialNotes(document, { trial: true })).toBe(0);
  });
});
