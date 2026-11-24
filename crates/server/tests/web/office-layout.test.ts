// 이름표 · 말풍선 · 말풍선 줄의 자리 잡기(server 23차 2 · 3 · 4 · 5번, 원본 DeskRPG `label-layout` 수치 —
// docs/research/deskrpg.md 11절): 말풍선 글 형식 · 후보 간격 `높이 + 6` · 첫 자리 `max(8, …)` · 글 없는 상태는 줄로 ·
// 쉬는 중은 아무것도 · 우선순위 · ①우선 이름표 ②줄 ③말풍선 ④나머지 이름표 · 줄 자리 네 모서리 순서와 크기.
import { describe, expect, it } from "vitest";
import { Chatter } from "../../web/src/office/chatter";
import {
  PRIORITY_OTHER,
  PRIORITY_SPEAKING,
  PRIORITY_WORKING,
  RAIL_CORNERS,
  RAIL_EXTRA_WIDTH,
  RAIL_GAP,
  RAIL_HEIGHT_RATIO,
  RAIL_MAX_HEIGHT,
  RAIL_RIGHT_INSET,
  RAIL_TOP,
  actorPriority,
  layoutOffice,
  railRect,
  type OfficeActor,
} from "../../web/src/office/layout";
import {
  BUBBLE_STACK_GAP,
  BUBBLE_TOP_MARGIN,
  SpeechBoard,
  bubbleText,
  bubbleWhere,
  bubbleWidth,
  placeBubbles,
  type HeadSpot,
  type Rect,
  type Size,
} from "../../web/src/office/speech";

const T0 = 1_000_000;
const SCREEN: Size = { width: 1280, height: 800 };
const BW = bubbleWidth(SCREEN.width); // 190
const TAG: Size = { width: 60, height: 20 };
const BUBBLE: Size = { width: BW, height: 40 };

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** 머리는 발에서 120px 위에 있는 담당. */
function actor(agent: string, x: number, footY: number, opts: Partial<OfficeActor> = {}): OfficeActor {
  return {
    agent,
    visible: true,
    foot: { x, y: footY },
    head: { x, y: footY - 120 },
    tag: TAG,
    bubble: null,
    railed: false,
    priority: PRIORITY_OTHER,
    ...opts,
  };
}

/** 발 아래 이름표 첫 자리. */
function tagBase(a: OfficeActor): Rect {
  return { x: a.foot.x - a.tag.width / 2, y: a.foot.y + 10, width: a.tag.width, height: a.tag.height };
}

/** 머리 위 말풍선 첫 자리. */
function bubbleBase(a: OfficeActor): Rect {
  const b = a.bubble as Size;
  return { x: a.head.x - b.width / 2, y: Math.max(8, a.head.y - b.height - 8), width: b.width, height: b.height };
}

function find(list: { agent: string; rect: Rect }[], agent: string): Rect | undefined {
  return list.find((p) => p.agent === agent)?.rect;
}

describe("말풍선 글 형식", () => {
  it("글 말풍선은 `이름 · 글`", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.say("risk", "손절선을 지킵니다", T0);
    expect(bubbleText("리스크 담당", board.shown("risk", T0)!)).toBe("리스크 담당 · 손절선을 지킵니다");
  });

  it("상태가 붙은 글은 `이름 · 💭 글`", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.say("risk", "손절선은", T0, "thinking");
    expect(bubbleText("리스크 담당", board.shown("risk", T0)!)).toBe("리스크 담당 · 💭 손절선은");
    board.say("news", "공시가", T0, "writing");
    expect(bubbleText("뉴스 담당", board.shown("news", T0)!)).toBe("뉴스 담당 · 💬 공시가");
  });

  it("글 없이 말하는 중이면 `이름 · ···`, 글 없는 상태면 `이름 · 💭`", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.typing("leader", T0);
    expect(bubbleText("팀장", board.shown("leader", T0)!)).toBe("팀장 · ···");
    board.status("strategy", "thinking", T0);
    expect(bubbleText("전략 담당", board.shown("strategy", T0)!)).toBe("전략 담당 · 💭");
  });
});

describe("말풍선이 그려지는 곳", () => {
  it("글 말풍선 · ··· · 잡담은 머리 위", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.say("risk", "손절선", T0);
    board.typing("leader", T0);
    board.smalltalk("news", "점심 뭐 드셨어요?", T0, T0 + 4_500);
    expect(bubbleWhere(board.shown("risk", T0))).toBe("head");
    expect(bubbleWhere(board.shown("leader", T0))).toBe("head");
    expect(bubbleWhere(board.shown("news", T0))).toBe("head");
  });

  it("글 없는 상태(생각 중 · 기다림 · 실패)는 머리 위가 아니라 말풍선 줄", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.status("risk", "thinking", T0);
    board.status("news", "waiting", T0);
    board.status("leader", "failed", T0);
    for (const id of ["risk", "news", "leader"]) expect(bubbleWhere(board.shown(id, T0))).toBe("rail");
  });

  it("쉬는 중은 말풍선도 줄도 없다", () => {
    // 검증: V14 V15
    const chatter = new Chatter();
    chatter.work("risk", "thinking", T0);
    expect(bubbleWhere(chatter.board.shown("risk", T0))).toBe("rail");
    chatter.work("risk", "idle", T0 + 1_000);
    expect(bubbleWhere(chatter.board.shown("risk", T0 + 1_000))).toBe("none");
    expect(bubbleWhere(undefined)).toBe("none");
  });
});

describe("자리 잡는 우선순위", () => {
  it("말하는 중(글 · ···) 80, 일하는 중 50, 그 밖 0", () => {
    // 검증: V15
    expect([PRIORITY_SPEAKING, PRIORITY_WORKING, PRIORITY_OTHER]).toEqual([80, 50, 0]);
    expect(actorPriority(true, "idle")).toBe(80);
    expect(actorPriority(true, "thinking")).toBe(80);
    expect(actorPriority(false, "thinking")).toBe(50);
    expect(actorPriority(false, "queued")).toBe(50);
    expect(actorPriority(false, "idle")).toBe(0);
  });
});

describe("말풍선 후보 간격(placeBubbles)", () => {
  /** 이름표는 발 아래(머리에서 멀리) 있다. */
  function head(agent: string, x: number, y: number): HeadSpot {
    return { agent, x, y, bubble: { width: 120, height: 40 }, label: { x: x - 30, y: y + 130, width: 60, height: 20 } };
  }

  it("위아래 후보는 높이 + 6씩 비킨다(위로)", () => {
    // 검증: V15
    expect(BUBBLE_STACK_GAP).toBe(6);
    const me = head("risk", 640, 400); // 첫 자리 (580, 352)
    // 가운데 줄 세 후보(x 514~766, y 352~392)를 모두 막는 띠.
    const band = head("wall", 100, 780);
    band.label = { x: 500, y: 352, width: 280, height: 40 };
    const result = placeBubbles([me, band], SCREEN);
    expect(result.placed.find((p) => p.agent === "risk")?.rect).toEqual({ x: 580, y: 352 - 46, width: 120, height: 40 });
  });

  it("위아래 후보는 높이 + 6씩 비킨다(아래로)", () => {
    // 검증: V15
    const me = head("risk", 640, 400);
    // 가운데 줄과 위 줄(y 306~392)을 모두 막는 띠.
    const band = head("wall", 100, 780);
    band.label = { x: 500, y: 300, width: 280, height: 92 };
    const result = placeBubbles([me, band], SCREEN);
    expect(result.placed.find((p) => p.agent === "risk")?.rect).toEqual({ x: 580, y: 352 + 46, width: 120, height: 40 });
  });

  it("첫 자리의 위쪽 끝은 max(8, 머리 y - 높이 - 8)", () => {
    // 검증: V15
    expect(BUBBLE_TOP_MARGIN).toBe(8);
    const result = placeBubbles([head("risk", 640, 30)], SCREEN);
    expect(result.placed).toEqual([{ agent: "risk", rect: { x: 580, y: 8, width: 120, height: 40 } }]);
  });
});

describe("말풍선 줄 자리", () => {
  it("원본 수치: 오른쪽 위 x = 화면폭 - 말풍선폭 - 18, y = 8, 폭 + 10, 높이 min(168, 화면높이 × 0.28), 간격 6", () => {
    // 검증: V15
    expect([RAIL_RIGHT_INSET, RAIL_TOP, RAIL_EXTRA_WIDTH, RAIL_MAX_HEIGHT, RAIL_HEIGHT_RATIO, RAIL_GAP]).toEqual([
      18, 8, 10, 168, 0.28, 6,
    ]);
    expect(RAIL_CORNERS).toEqual(["top-right", "top-left", "bottom-left", "bottom-right"]);
    expect(railRect("top-right", SCREEN, 190)).toEqual({ x: 1280 - 190 - 18, y: 8, width: 200, height: 168 });
    // 낮은 화면: 높이 = 화면높이 × 0.28
    expect(railRect("top-right", { width: 400, height: 500 }, 190)).toEqual({ x: 192, y: 8, width: 200, height: 140 });
  });

  it("다른 모서리도 화면 가장자리에서 같은 여백 8px", () => {
    // 검증: V15
    expect(railRect("top-left", SCREEN, 190)).toEqual({ x: 8, y: 8, width: 200, height: 168 });
    expect(railRect("bottom-left", SCREEN, 190)).toEqual({ x: 8, y: 800 - 168 - 8, width: 200, height: 168 });
    expect(railRect("bottom-right", SCREEN, 190)).toEqual({ x: 1072, y: 800 - 168 - 8, width: 200, height: 168 });
  });
});

describe("한 프레임의 자리 잡기(layoutOffice)", () => {
  it("말풍선 · 줄이 없으면 이름표만 발 아래에 놓고 줄은 없다", () => {
    // 검증: V15
    const a = actor("risk", 640, 500);
    const layout = layoutOffice([a], SCREEN);
    expect(layout.tags).toEqual([{ agent: "risk", rect: tagBase(a) }]);
    expect(layout.bubbles).toEqual([]);
    expect(layout.rail).toBeNull();
    expect(layout.hidden).toEqual([]);
  });

  it("말풍선은 머리 위 첫 자리, 글 없는 상태는 오른쪽 위 줄로", () => {
    // 검증: V14 V15
    const talker = actor("risk", 640, 500, { bubble: BUBBLE, priority: 80 });
    const thinker = actor("strategy", 300, 500, { railed: true, priority: 50 });
    const layout = layoutOffice([talker, thinker], SCREEN);
    expect(find(layout.bubbles, "risk")).toEqual(bubbleBase(talker));
    expect(find(layout.bubbles, "strategy")).toBeUndefined();
    expect(layout.rail).toEqual({ corner: "top-right", rect: railRect("top-right", SCREEN, BW), agents: ["strategy"] });
  });

  it("우선순위 80 이상의 이름표가 오른쪽 위를 막으면 줄은 왼쪽 위 → 왼쪽 아래 → 오른쪽 아래", () => {
    // 검증: V15
    const thinker = actor("strategy", 640, 500, { railed: true, priority: 50 });
    // 이름표 첫 자리가 각 모서리의 줄 자리에 걸리는 우선순위 80 담당들.
    const atTopRight = actor("risk", 1150, 30, { priority: 80 });
    const atTopLeft = actor("news", 100, 30, { priority: 80 });
    const atBottomLeft = actor("leader", 100, 700, { priority: 80 });
    expect(layoutOffice([thinker, atTopRight], SCREEN).rail?.corner).toBe("top-left");
    expect(layoutOffice([thinker, atTopRight, atTopLeft], SCREEN).rail?.corner).toBe("bottom-left");
    expect(layoutOffice([thinker, atTopRight, atTopLeft, atBottomLeft], SCREEN).rail?.corner).toBe("bottom-right");
  });

  it("우선순위가 낮은 이름표는 줄을 옮기지 않는다(줄 다음에 놓는다)", () => {
    // 검증: V15
    const thinker = actor("strategy", 640, 500, { railed: true, priority: 50 });
    const idleTopRight = actor("risk", 1150, 30, { priority: 0 });
    const layout = layoutOffice([thinker, idleTopRight], SCREEN);
    expect(layout.rail?.corner).toBe("top-right");
  });

  it("① 우선 이름표 다음에 ③ 말풍선: 말풍선은 우선 이름표를 덮지 않는다", () => {
    // 검증: V15
    const talker = actor("risk", 640, 500, { bubble: BUBBLE, priority: 80 }); // 말풍선 첫 자리 y 332~372
    // 이름표 첫 자리(y 350~370)가 talker 말풍선 첫 자리에 걸리는 일하는 중 담당.
    const worker = actor("news", 640, 340, { priority: 80 });
    const layout = layoutOffice([talker, worker], SCREEN);
    expect(find(layout.tags, "news")).toEqual(tagBase(worker));
    const bubble = find(layout.bubbles, "risk");
    expect(bubble).toBeDefined();
    expect(bubble).not.toEqual(bubbleBase(talker));
    expect(overlaps(bubble as Rect, tagBase(worker))).toBe(false);
  });

  it("③ 말풍선 다음에 ④ 나머지 이름표: 낮은 이름표가 말풍선을 피해 내려간다", () => {
    // 검증: V15
    const talker = actor("risk", 640, 500, { bubble: BUBBLE, priority: 80 }); // 말풍선 첫 자리 y 332~372
    const idle = actor("news", 640, 340, { priority: 0 }); // 이름표 첫 자리 y 350~370
    const layout = layoutOffice([idle, talker], SCREEN);
    expect(find(layout.bubbles, "risk")).toEqual(bubbleBase(talker));
    const tag = find(layout.tags, "news");
    expect(tag).toBeDefined();
    expect(tag).not.toEqual(tagBase(idle));
    expect(overlaps(tag as Rect, bubbleBase(talker))).toBe(false);
  });

  it("② 줄 다음에 ③ 말풍선: 줄이 있으면 말풍선은 줄을 덮지 않는다", () => {
    // 검증: V15
    const thinker = actor("strategy", 300, 600, { railed: true, priority: 50 });
    // 머리 위 9후보(x 974~1226, y 66~198)가 모두 오른쪽 위 줄(x 1072~1272, y 8~176)에 걸리는 말하는 담당.
    const talker = actor("risk", 1100, 280, { bubble: { width: 120, height: 40 }, priority: 80 });
    // 줄이 없으면 그 자리 그대로 머리 위에 뜬다.
    expect(find(layoutOffice([talker], SCREEN).bubbles, "risk")).toEqual(bubbleBase(talker));
    const layout = layoutOffice([thinker, talker], SCREEN);
    expect(layout.rail?.corner).toBe("top-right");
    expect(find(layout.bubbles, "risk")).toBeUndefined();
    expect(layout.rail?.agents).toEqual(["strategy", "risk"]);
  });

  it("자리가 없는 말풍선은 줄로 보낸다", () => {
    // 검증: V15
    // 낮은 화면 꼭대기 가운데에 큰 말풍선 둘: 둘째(id 순서로 뒤)는 모든 후보가 첫째와 겹치거나 화면 밖이다.
    const screen: Size = { width: 1280, height: 250 };
    const big: Size = { width: 190, height: 120 };
    const first = actor("leader", 640, 200, { head: { x: 640, y: 130 }, bubble: big, priority: 80 });
    const second = actor("risk", 650, 200, { head: { x: 650, y: 130 }, bubble: big, priority: 80 });
    const layout = layoutOffice([second, first], screen);
    expect(layout.bubbles).toEqual([{ agent: "leader", rect: { x: 545, y: 8, width: 190, height: 120 } }]);
    expect(layout.rail?.agents).toEqual(["risk"]);
    expect(layout.rail?.rect).toEqual(railRect("top-right", screen, 190));
  });

  it("말풍선 넓이 합이 화면의 24%를 넘으면 모두 줄로", () => {
    // 검증: V15
    const small: Size = { width: 400, height: 300 }; // 24% = 28800
    const big: Size = { width: 190, height: 80 }; // 15200
    const two = [
      actor("risk", 100, 290, { bubble: big, priority: 80 }),
      actor("news", 300, 290, { bubble: big, priority: 80 }),
    ];
    const layout = layoutOffice(two, small);
    expect(layout.bubbles).toEqual([]);
    expect([...(layout.rail?.agents ?? [])].sort()).toEqual(["news", "risk"]);
  });

  it("화면 밖 담당은 이름표 · 말풍선 모두 숨긴다", () => {
    // 검증: V15
    const away = actor("risk", 640, 500, { visible: false, bubble: BUBBLE, priority: 80 });
    const here = actor("news", 300, 500);
    const layout = layoutOffice([away, here], SCREEN);
    expect(layout.hidden).toEqual(["risk"]);
    expect(find(layout.tags, "risk")).toBeUndefined();
    expect(find(layout.bubbles, "risk")).toBeUndefined();
    expect(layout.rail).toBeNull();
    expect(find(layout.tags, "news")).toEqual(tagBase(here));
  });

  it("같은 우선순위면 id 순서로 자리를 잡는다", () => {
    // 검증: V15
    const b = actor("news", 640, 500, { priority: 50 });
    const a = actor("leader", 640, 500, { priority: 50 });
    const layout = layoutOffice([b, a], SCREEN);
    expect(find(layout.tags, "leader")).toEqual(tagBase(a));
    expect(find(layout.tags, "news")).not.toEqual(tagBase(b));
  });
});
