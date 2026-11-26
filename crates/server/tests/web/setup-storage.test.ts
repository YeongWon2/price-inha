// 61차(결정 31): 설정 화면 진단 표에 기록 용량 한 줄(`storage`). 값 계산은 서버(Rust)가 하고 화면은 받은 글을 그린다.
// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { diagnosisTable } from "../../web/src/setup/extras";

const LINE = "기록: 12.3 MB, 하루 약 45 KB, 10년 약 0.2 GB";

describe("진단 표의 기록 용량 줄", () => {
  it("저장소 줄 바로 뒤에 기록 용량 줄을 받은 글 그대로 알림으로 그린다", () => {
    const block = diagnosisTable({
      items: [
        { key: "store", label: "저장소", status: "ok", detail: "저장소 판 12", todo: null },
        { key: "storage", label: "기록 용량", status: "info", detail: LINE, todo: null },
        { key: "keys", label: "키 등록", status: "ok", detail: "증권사 있음", todo: null },
      ],
    });
    const rows = Array.from(block.querySelectorAll<HTMLTableRowElement>("tr[data-key]"));
    expect(rows.map((row) => row.dataset.key)).toEqual(["store", "storage", "keys"]);
    const row = block.querySelector<HTMLTableRowElement>('tr[data-key="storage"]');
    expect(row?.cells[0]?.textContent).toBe("기록 용량");
    expect(row?.cells[1]?.textContent).toBe("알림");
    expect(row?.cells[2]?.textContent).toBe(LINE);
    expect(block.textContent).not.toContain("undefined");
  });
});
