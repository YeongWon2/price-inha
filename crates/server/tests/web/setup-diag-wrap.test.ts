// 다듬기 E 5번: 설정 화면 진단 표의 긴 "본 것" · "할 일" 칸은 한 줄로 잘려 가로로 밀리지 않고 줄바꿈된다.
// 표 칸에 `setup-diag-text`를 달고, 설정 화면 모양(setup.css)이 그 칸을 줄바꿈(`white-space: normal` ·
// `overflow-wrap: anywhere`)으로 둔다. 진단 항목 · 칸 순서는 그대로다.
// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { diagnosisTable } from "../../web/src/setup/extras";
import { CSS_SOURCES } from "./source-graph";

const css = CSS_SOURCES["../../web/src/setup/setup.css"];

const LONG = "증권사 키 있음 · AI 연결 없음 · DART 키(선택) 없음 · ECOS 키(선택) 없음 · FRED 키(선택) 없음 · 백업 GitHub 토큰(선택) 없음";

describe("진단 표 줄바꿈", () => {
  it("본 것 · 할 일 칸에 줄바꿈 class를 단다", () => {
    const block = diagnosisTable({
      items: [{ key: "keys", label: "키 등록", status: "warn", detail: LONG, todo: "설정 화면에서 등록합니다" }],
    });
    const row = block.querySelector<HTMLTableRowElement>('tr[data-key="keys"]');
    expect(row).not.toBeNull();
    expect(row?.cells[2]?.classList.contains("setup-diag-text")).toBe(true);
    expect(row?.cells[2]?.textContent).toBe(LONG);
    expect(row?.cells[3]?.classList.contains("setup-diag-text")).toBe(true);
    expect(row?.cells[3]?.getAttribute("data-role")).toBe("todo");
  });

  it("설정 화면 모양은 그 칸을 줄바꿈으로 둔다", () => {
    const rule = /\.setup-diag-text\s*\{([^}]*)\}/.exec(css);
    expect(rule, "setup-diag-text 규칙이 있다").not.toBeNull();
    expect(rule?.[1]).toMatch(/white-space:\s*normal/);
    expect(rule?.[1]).toMatch(/overflow-wrap:\s*anywhere/);
  });
});
