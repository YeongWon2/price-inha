// 공통 부품(server 26차 2번): 카드 · 표 · 단추(기본 · 강조 · 위험) · 배지(상태) · 입력 칸 · 빈 상태 · 불러오는 중 · 오류 줄.
// 순수 DOM 함수이고, 글은 글자로만 들어간다(HTML로 해석하지 않는다).
import { describe, expect, it } from "vitest";
import { badge, button, card, emptyState, errorLine, field, loading, table } from "../../web/src/ui/parts";

const TRAP = '<img src="x" onerror="alert(1)"><b>굵게</b>';

describe("공통 부품", () => {
  it("단추는 type=button이고 종류(기본 · 강조 · 위험)가 data-kind에 적힌다", () => {
    // 검증: V76 F12
    const plain = button("저장");
    expect(plain.tagName).toBe("BUTTON");
    expect(plain.type).toBe("button");
    expect(plain.textContent).toBe("저장");
    expect(plain.dataset.kind).toBe("default");
    expect(button("시작", "accent").dataset.kind).toBe("accent");
    expect(button("멈춤", "danger").dataset.kind).toBe("danger");
  });

  it("배지는 상태가 data-tone에 적힌다(안 주면 neutral)", () => {
    // 검증: V76 F12
    expect(badge("대기").dataset.tone).toBe("neutral");
    for (const tone of ["success", "warning", "danger"] as const) {
      const made = badge("상태", tone);
      expect(made.dataset.tone).toBe(tone);
      expect(made.textContent).toBe("상태");
    }
  });

  it("카드는 section이고 첫 자식이 제목 h2, 그 뒤에 내용이 차례로 붙는다", () => {
    // 검증: F12
    const first = document.createElement("p");
    first.textContent = "하나";
    const second = document.createElement("p");
    second.textContent = "둘";
    const made = card("잔고", first, second);
    expect(made.tagName).toBe("SECTION");
    expect(made.firstElementChild?.tagName).toBe("H2");
    expect(made.firstElementChild?.textContent).toBe("잔고");
    expect([...made.children].slice(1)).toEqual([first, second]);
  });

  it("표는 머리 칸이 th[scope=col]이고 줄 · 칸이 차례대로 들어간다", () => {
    // 검증: F12
    const made = table(["종목", "수량"], [["005930", "3"], ["000660", "1"]]);
    expect(made.tagName).toBe("TABLE");
    const heads = [...made.querySelectorAll("thead th")];
    expect(heads.map((th) => th.textContent)).toEqual(["종목", "수량"]);
    for (const th of heads) expect(th.getAttribute("scope")).toBe("col");
    const rows = [...made.querySelectorAll("tbody tr")].map((tr) => [...tr.querySelectorAll("td")].map((td) => td.textContent));
    expect(rows).toEqual([["005930", "3"], ["000660", "1"]]);
    expect(table(["종목"], []).querySelectorAll("tbody tr").length).toBe(0);
  });

  it("입력 칸은 label[for]가 같은 id의 input을 가리킨다", () => {
    // 검증: F12
    const made = field("하루 손실 한도", "f-loss");
    const label = made.querySelector("label");
    const input = made.querySelector("input");
    expect(label?.textContent).toBe("하루 손실 한도");
    expect(label?.htmlFor).toBe("f-loss");
    expect(input?.id).toBe("f-loss");
    expect(input?.type).toBe("text");
    expect(field("숫자", "f-n", "number").querySelector("input")?.type).toBe("number");
  });

  it("빈 상태 · 불러오는 중은 status, 오류 줄은 alert로 알린다", () => {
    // 검증: F12
    const empty = emptyState("준비 중");
    expect(empty.getAttribute("role")).toBe("status");
    expect(empty.textContent).toContain("준비 중");
    const wait = loading();
    expect(wait.getAttribute("role")).toBe("status");
    expect(wait.getAttribute("aria-busy")).toBe("true");
    expect((wait.textContent ?? "").trim().length).toBeGreaterThan(0);
    const bad = errorLine("불러오지 못했습니다");
    expect(bad.getAttribute("role")).toBe("alert");
    expect(bad.textContent).toContain("불러오지 못했습니다");
  });

  it("어느 부품도 받은 글을 HTML로 해석하지 않는다", () => {
    // 검증: F12 D11
    const made = [
      button(TRAP),
      badge(TRAP),
      card(TRAP),
      table([TRAP], [[TRAP]]),
      field(TRAP, "f-trap"),
      emptyState(TRAP),
      loading(TRAP),
      errorLine(TRAP),
    ];
    for (const node of made) {
      expect(node.querySelector("img, b")).toBeNull();
      expect(node.textContent).toContain(TRAP);
    }
  });
});
