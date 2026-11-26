// 승인 카드 보강(server 50차 · C24 · R4-4 · V55): 주문 승인 카드(사무실 · 폰)에 담당 투표 수("강세 n · 약세 n ·
// 중립 n · 기권 n")와 한도 적용 전 → 후("15주 → 10주(종목 비중 한도)")가 보인다. 두 칸이 없으면 자리가 숨는다.
// 승인 지문 · 잠금은 그대로다(보낸 지문은 카드에 받은 것 그대로).
import { afterEach, describe, expect, it, vi } from "vitest";
import { applyApproval, decideApproval } from "../../web/src/office/approval";
import { approvalPath, decidePath } from "../../web/src/paths";
import { extrasLines, renderApprovalExtras } from "../../web/src/ui/approval-extras";

const NUMBER = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const FULL = "a1b2c3" + "d".repeat(58);
const VOTES = { bull: 4, bear: 0, neutral: 1, abstain: 1 };
const CLAMPS = [
  { limit: "risk_per_trade", before: "15", after: "12", reason: "1회 위험 한도" },
  { limit: "symbol_weight", before: "12", after: "9", reason: "종목 비중 한도" },
];

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("승인 카드 보강 글", () => {
  it("투표 수는 넷 다, 한도 줄은 적용한 차례로 '전 → 후(사유)'가 된다", () => {
    // 검증: V55 R4-4 C24
    expect(extrasLines({ votes: VOTES, clamps: CLAMPS })).toEqual({
      votes: "강세 4 · 약세 0 · 중립 1 · 기권 1",
      clamps: ["15주 → 12주(1회 위험 한도)", "12주 → 9주(종목 비중 한도)"],
    });
  });

  it("두 칸이 없거나 모양이 틀리면 글이 없다", () => {
    // 검증: C24 E53
    for (const wire of [null, {}, { votes: null, clamps: null }, { votes: { bull: "x" }, clamps: "x" }]) {
      expect(extrasLines(wire)).toEqual({ votes: null, clamps: [] });
    }
    expect(extrasLines({ clamps: [{ limit: "x" }, CLAMPS[1]] }).clamps).toEqual(["12주 → 9주(종목 비중 한도)"]);
  });

  it("자리에 투표 줄과 한도 줄을 그리고, 보일 것이 없으면 숨긴다", () => {
    // 검증: V55 C24
    const box = document.createElement("div");
    renderApprovalExtras(box, { votes: VOTES, clamps: CLAMPS });
    expect(box.hidden).toBe(false);
    expect(box.querySelector('[data-part="votes"]')?.textContent).toContain("강세 4 · 약세 0 · 중립 1 · 기권 1");
    const clamps = [...box.querySelectorAll('[data-part="clamp"]')].map((p) => p.textContent);
    expect(clamps).toHaveLength(2);
    expect(clamps[1]).toContain("12주 → 9주(종목 비중 한도)");
    renderApprovalExtras(box, {});
    expect(box.hidden).toBe(true);
    expect(box.childElementCount).toBe(0);
    renderApprovalExtras(null, { votes: VOTES });
  });
});

describe("사무실 승인 카드 보강", () => {
  it("office.approval의 투표 수와 전 → 후가 카드에 보이고, 지문 앞 6자 · 승인 보내기는 그대로다", async () => {
    // 검증: V55 C24 R4-4
    const root = document.createElement("section");
    document.body.replaceChildren(root);
    const fetcher = vi.fn(async (input: string, init?: RequestInit) => {
      if ((init?.method ?? "GET") === "GET" && input === approvalPath(NUMBER)) {
        return json(200, { number: NUMBER, fingerprint: FULL, votes: VOTES, clamps: CLAMPS });
      }
      return json(200, { result: "rejected", reason: "" });
    });
    const payload = { number: NUMBER, summary: "000660 · 금액 한도", short_code: "a1b2c3", status: "pending" };
    applyApproval(root, { ...payload, votes: VOTES, clamps: CLAMPS }, fetcher);
    const card = root.querySelector<HTMLElement>(`[data-approval="${NUMBER}"]`);
    expect(card).not.toBeNull();
    const extras = card?.querySelector<HTMLElement>('[data-part="extras"]');
    expect(extras?.hidden).toBe(false);
    expect(extras?.querySelector('[data-part="votes"]')?.textContent).toContain("강세 4 · 약세 0 · 중립 1 · 기권 1");
    expect(extras?.textContent).toContain("12주 → 9주(종목 비중 한도)");
    expect(card?.querySelector('[data-part="code"]')?.textContent).toBe("a1b2c3");
    expect(card?.querySelector('[data-part="summary"]')?.textContent).toBe("000660 · 금액 한도");
    // 같은 번호의 다음 이벤트(두 칸 없음)는 보강 자리를 숨긴다(한 장 그대로).
    applyApproval(root, payload, fetcher);
    expect(root.querySelectorAll("[data-approval]")).toHaveLength(1);
    expect(card?.querySelector<HTMLElement>('[data-part="extras"]')?.hidden).toBe(true);
    // 보내는 지문은 카드가 받은 전체 지문 그대로다.
    expect(await decideApproval(root, NUMBER, "reject", fetcher)).toBe("sent");
    const post = fetcher.mock.calls.find(([url]) => url === decidePath(NUMBER, "reject"));
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ fingerprint: FULL });
  });
});
