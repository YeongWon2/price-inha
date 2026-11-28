// 관심 종목 표의 거래량(V32): 서버가 시세 묶음의 종목마다 준 그날 누적 거래량(`volume`, 십진수 글자)을
// 쉬운 말("1.2만주")로 그리기만 한다. 모르면(칸 없음 · 읽을 수 없음) 비워 둔다(0으로 지어내지 않는다).
import { describe, expect, it } from "vitest";
import { fillVolume, volumeText } from "../../web/src/setup/volume";

const pages = import.meta.glob<string>("../../web/index.html", { query: "?raw", import: "default", eager: true });
const html = Object.values(pages)[0] ?? "";

describe("거래량 글자", () => {
  it("만 아래는 자릿수 쉼표와 주로 적는다", () => {
    // 검증: V32
    expect(volumeText("0")).toBe("0주");
    expect(volumeText("1234")).toBe("1,234주");
    expect(volumeText("9999")).toBe("9,999주");
  });

  it("만 · 억은 소수 한 자리까지 쉬운 말로 줄인다", () => {
    // 검증: V32
    expect(volumeText("12345")).toBe("1.2만주");
    expect(volumeText("3521000")).toBe("352.1만주");
    expect(volumeText("50000")).toBe("5만주");
    expect(volumeText("120000000")).toBe("1.2억주");
    expect(volumeText("1500000000")).toBe("15억주");
  });

  it("반올림이 다음 단위에 닿으면 다음 단위로 적는다", () => {
    // 검증: V32
    expect(volumeText("99999")).toBe("10만주");
    expect(volumeText("99999999")).toBe("1억주");
  });

  it("모르는 값은 비운다", () => {
    // 검증: V32
    for (const bad of [undefined, null, "", "많음", "-1", 3521000]) {
      expect(volumeText(bad)).toBe("");
    }
  });
});

describe("관심 종목 표의 거래량 칸", () => {
  it("표 머리에 현재가 다음 거래량 칸이 있다", () => {
    // 검증: V32
    const doc = new DOMParser().parseFromString(html, "text/html");
    const heads = Array.from(doc.querySelectorAll("#quotes")[0]?.closest("table")?.querySelectorAll("thead th") ?? [])
      .map((th) => th.textContent?.trim());
    expect(heads.slice(0, 3)).toEqual(["종목", "현재가", "거래량"]);
  });

  it("시세 묶음이 올 때마다 그 칸을 서버가 준 값으로 다시 그린다", () => {
    // 검증: V32
    const cell = document.createElement("td");
    fillVolume(cell, { symbol: "005930", price: "72000", volume: "3521000" });
    expect(cell.textContent).toBe("352.1만주");
    fillVolume(cell, { symbol: "005930", price: "72100", volume: "3530000" });
    expect(cell.textContent).toBe("353만주");
    fillVolume(cell, { symbol: "005930", price: "72100" });
    expect(cell.textContent, "모르면 비운다").toBe("");
  });
});
