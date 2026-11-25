// 승인 ✋ 말풍선(server 34차 1번 · 5번, additions.md R5-10 · C27 · E49 · V62 — 원본 동작: docs/research/agent-office-refs.md
// 1-B · F 1): 승인 이벤트에 ✋ · 시간이 지나도 남음 · 끝남 이벤트에 사라짐 · 다시 읽기로 채움과 맞춤 · 누르면 카드 열기 ·
// 담당에 여럿이면 가장 오래된 것과 개수. 장면 없이 `src/office/approval-bubbles.ts`를 돌려 본다. 시각은 가짜 시계(밀리초)다.
import { describe, expect, it, vi } from "vitest";
import { ApprovalBubbles, type WaitingApproval } from "../../web/src/office/approval-bubbles";
import { markOf } from "../../web/src/office/legend";
import { SPEECH_LIFE_MS } from "../../web/src/office/speech";

const T0 = 1_000_000;
const N = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const M = "b7c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5";
const K = "0a1b2c3d4e5f60718293a4b5c6d7e8f9";

/** 34차 모양의 승인 이벤트(agent · state가 있다). */
function waitingEvent(number: string, agent = "trader", over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    number,
    summary: "하이닉스 5주 사기",
    short_code: "a1b2c3",
    status: "pending",
    agent,
    state: "waiting",
    ...over,
  };
}

/** 다시 읽기(`pending_approvals`)의 한 줄. */
function row(number: string, agent: string, createdAt: number): Record<string, unknown> {
  return { agent, number, created_at: createdAt, summary: "개선안 반영 요청", short_code: "c3d4e5" };
}

describe("승인 ✋ 말풍선 — 이벤트", () => {
  it("승인을 기다리는 이벤트가 오면 요청한 담당 머리 위에 ✋가 뜬다(범례와 같은 글자 · 색)", () => {
    // 검증: V62 R5-10 C27
    const board = new ApprovalBubbles();
    expect(board.apply(waitingEvent(N, "trader"), T0)).toBe("new");
    const hand = board.bubble("trader", T0);
    expect(hand?.agent).toBe("trader");
    expect(hand?.oldest.number).toBe(N);
    expect(hand?.oldest.summary).toBe("하이닉스 5주 사기");
    expect(hand?.oldest.short_code).toBe("a1b2c3");
    expect(hand?.count).toBe(1);
    expect(hand?.text).toBe(markOf("approval").glyph);
    expect(markOf("approval").glyph).toBe("✋");
    expect(hand?.color).toBe(markOf("approval").color);
    expect(board.bubble("leader", T0)).toBeUndefined();
    expect(board.bubble("review", T0)).toBeUndefined();
  });

  it("✋는 시간이 지나도 사라지지 않는다(다른 말풍선의 수명과 무관)", () => {
    // 검증: V62 N12
    const board = new ApprovalBubbles();
    board.apply(waitingEvent(N, "review"), T0);
    expect(board.bubble("review", T0 + SPEECH_LIFE_MS + 1)?.oldest.number).toBe(N);
    expect(board.bubble("review", T0 + 24 * 60 * 60 * 1000)?.oldest.number).toBe(N);
    expect(board.bubbles(T0 + 24 * 60 * 60 * 1000).map((b) => b.agent)).toEqual(["review"]);
  });

  it("같은 요청의 이벤트가 또 오면 새 요청이 아니고 ✋는 하나다", () => {
    // 검증: V62 C27
    const board = new ApprovalBubbles();
    expect(board.apply(waitingEvent(N), T0)).toBe("new");
    expect(board.apply(waitingEvent(N), T0 + 5_000)).toBe("known");
    expect(board.bubble("trader", T0 + 5_000)?.count).toBe(1);
    expect(board.waiting().map((w) => w.number)).toEqual([N]);
  });

  it("답함 · 만료 이벤트가 오면 그 ✋가 사라진다", () => {
    // 검증: V62 C27
    const board = new ApprovalBubbles();
    board.apply(waitingEvent(N, "trader"), T0);
    board.apply(waitingEvent(M, "leader"), T0 + 1);
    expect(board.apply(waitingEvent(N, "trader", { status: "approved", state: "answered" }), T0 + 2)).toBe("closed");
    expect(board.bubble("trader", T0 + 2)).toBeUndefined();
    expect(board.apply(waitingEvent(M, "leader", { status: "expired", state: "expired" }), T0 + 3)).toBe("closed");
    expect(board.bubble("leader", T0 + 3)).toBeUndefined();
    expect(board.waiting()).toEqual([]);
  });

  it("담당 · 상태 칸이 없는 옛 이벤트는 status로 보고 팀장에게 띄운다", () => {
    // 검증: V62 C27
    const board = new ApprovalBubbles();
    const old = { number: N, summary: "삼성전자 10주 사기", short_code: "a1b2c3", status: "pending" };
    expect(board.apply(old, T0)).toBe("new");
    expect(board.bubble("leader", T0)?.oldest.number).toBe(N);
    expect(board.apply({ ...old, status: "rejected" }, T0 + 1)).toBe("closed");
    expect(board.bubble("leader", T0 + 1)).toBeUndefined();
  });

  it("모르는 담당이면 팀장에게 띄우고, 모양이 맞지 않는 몸통은 무시한다", () => {
    // 검증: V62 C27
    const board = new ApprovalBubbles();
    expect(board.apply(waitingEvent(N, "boss"), T0)).toBe("new");
    expect(board.bubble("leader", T0)?.oldest.number).toBe(N);
    expect(board.apply(null, T0)).toBe("ignored");
    expect(board.apply({ number: "" , status: "pending" }, T0)).toBe("ignored");
    expect(board.apply("office.approval", T0)).toBe("ignored");
    expect(board.waiting().map((w) => w.number)).toEqual([N]);
  });
});

describe("승인 ✋ 말풍선 — 여럿", () => {
  it("한 담당에 여럿이면 ✋ 하나에 가장 오래된 요청과 개수를 보인다", () => {
    // 검증: V62 C27
    const board = new ApprovalBubbles();
    board.apply(waitingEvent(N, "trader"), T0);
    board.apply(waitingEvent(M, "trader"), T0 + 1_000);
    board.apply(waitingEvent(K, "review"), T0 + 2_000);
    const hand = board.bubble("trader", T0 + 3_000);
    expect(hand?.oldest.number).toBe(N);
    expect(hand?.count).toBe(2);
    expect(hand?.text.startsWith(markOf("approval").glyph)).toBe(true);
    expect(hand?.text.includes("2")).toBe(true);
    expect(board.bubble("review", T0 + 3_000)?.count).toBe(1);
    expect(board.bubbles(T0 + 3_000).map((b) => b.agent).sort()).toEqual(["review", "trader"]);
    board.apply(waitingEvent(N, "trader", { status: "approved", state: "answered" }), T0 + 4_000);
    const rest = board.bubble("trader", T0 + 4_000);
    expect(rest?.oldest.number).toBe(M);
    expect(rest?.count).toBe(1);
    expect(rest?.text).toBe(markOf("approval").glyph);
  });

  it("누르면 그 담당의 가장 오래된 요청으로 카드 열기를 한 번 부르고 ✋는 남는다", () => {
    // 검증: V62 R5-10
    const board = new ApprovalBubbles();
    board.apply(waitingEvent(N, "trader"), T0);
    board.apply(waitingEvent(M, "trader"), T0 + 1_000);
    const open = vi.fn<(approval: WaitingApproval) => void>();
    expect(board.press("trader", open)).toBe(true);
    expect(open).toHaveBeenCalledTimes(1);
    expect(open.mock.calls[0][0].number).toBe(N);
    expect(open.mock.calls[0][0].summary).toBe("하이닉스 5주 사기");
    expect(board.bubble("trader", T0 + 2_000)?.count).toBe(2);
    expect(board.press("risk", open)).toBe(false);
    expect(open).toHaveBeenCalledTimes(1);
  });
});

describe("승인 ✋ 말풍선 — 다시 읽기", () => {
  it("다시 열면 pending_approvals로 ✋를 채우고 오래된 차례는 만든 시각으로 정한다", () => {
    // 검증: V62 E49 C27
    const board = new ApprovalBubbles();
    board.sync([row(M, "review", 2_000), row(N, "review", 1_000), row(K, "leader", 3_000)]);
    expect(board.waiting().map((w) => w.number)).toEqual([N, M, K]);
    const hand = board.bubble("review", T0);
    expect(hand?.oldest.number).toBe(N);
    expect(hand?.oldest.at).toBe(1_000);
    expect(hand?.count).toBe(2);
    expect(board.bubble("leader", T0)?.oldest.short_code).toBe("c3d4e5");
  });

  it("채운 요청의 이벤트가 다시 와도 새 요청이 아니다(알림음을 내지 않을 일)", () => {
    // 검증: V62 E49
    const board = new ApprovalBubbles();
    board.sync([row(N, "trader", 1_000)]);
    expect(board.apply(waitingEvent(N, "trader"), T0)).toBe("known");
    expect(board.apply(waitingEvent(M, "trader"), T0 + 1)).toBe("new");
  });

  it("다시 읽은 목록에 없는 요청은 지운다(끝남 이벤트를 놓쳐도 ✋가 남지 않는다)", () => {
    // 검증: V62 E49 C27
    const board = new ApprovalBubbles();
    board.apply(waitingEvent(N, "trader"), T0);
    board.apply(waitingEvent(M, "leader"), T0 + 1);
    board.sync([row(M, "leader", 500)]);
    expect(board.bubble("trader", T0 + 2)).toBeUndefined();
    expect(board.bubble("leader", T0 + 2)?.oldest.number).toBe(M);
    board.sync([]);
    expect(board.waiting()).toEqual([]);
    expect(board.bubbles(T0 + 3)).toEqual([]);
  });

  it("목록이 배열이 아니면 아무것도 바꾸지 않고 모양이 맞지 않는 줄은 건너뛴다", () => {
    // 검증: V62 C27
    const board = new ApprovalBubbles();
    board.apply(waitingEvent(N, "trader"), T0);
    board.sync(undefined);
    expect(board.waiting().map((w) => w.number)).toEqual([N]);
    board.sync([row(N, "trader", 1_000), { number: 7 }, null, row(M, "nobody", 2_000)]);
    const numbers = board.waiting().map((w) => w.number);
    expect(numbers).toEqual([N, M]);
    expect(board.bubble("leader", T0)?.oldest.number).toBe(M);
  });
});
