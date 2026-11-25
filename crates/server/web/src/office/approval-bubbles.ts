// 승인 ✋ 말풍선의 상태(server 34차 1번 · 5번, additions.md R5-10 · C27 · E49 · V62 — 원본 동작:
// docs/research/agent-office-refs.md 1-B · F 1, pixel-agents의 권한 말풍선은 지울 때까지 남는다).
// 장면 없이 시험할 수 있게 순수 클래스로 둔다. 시각은 부르는 쪽이 넘기는 밀리초(`now`)다.
//
// - 이벤트: `office.approval` `{number, summary, short_code, status, agent?, state?}`(C27). `state`가 `"waiting"`이면
//   (없으면 `status`가 `"pending"`이면) 기다림, `"answered"` · `"expired"`이면(없으면 `status`가 `"pending"`이 아니면) 끝남.
//   `agent`가 담당 id가 아니거나 없으면 팀장(`"leader"`)에게 띄운다.
// - 다시 읽기: `GET /api/office`의 `pending_approvals` `[{agent, number, created_at, summary, short_code}]`로
//   통째로 맞춘다(목록에 없는 것은 지운다 — 이벤트를 놓쳐도 끝난 ✋가 남지 않는다). 다시 읽어 채운 것은 "새 요청"이
//   아니다(알림음을 내지 않는다, E49).
// - ✋는 시간으로 사라지지 않는다. 담당마다 하나(가장 오래된 요청)를 보이고 개수를 함께 준다.
// - 글자 · 색은 legend.ts의 `approval` 표시를 쓴다.

import type { AgentId } from "./actor";
import { markOf } from "./legend";

/** 기다리는 승인 하나. `at`은 오래된 차례를 정하는 밀리초(이벤트는 받은 `now`, 다시 읽기는 `created_at`). */
export interface WaitingApproval {
  number: string;
  agent: AgentId;
  at: number;
  summary: string;
  short_code: string;
}

/** 담당 하나의 ✋ 말풍선: 가장 오래된 요청 · 그 담당에 기다리는 개수 · 보일 글자 · 색. */
export interface HandBubble {
  agent: AgentId;
  /** 가장 오래된 요청(누르면 이것의 카드를 연다). */
  oldest: WaitingApproval;
  count: number;
  /** 하나면 ✋ 글자 그대로, 여럿이면 ✋ 글자로 시작하고 개수가 붙는다. */
  text: string;
  color: string;
}

/** 이벤트 하나를 반영한 결과: 새 요청(알림음을 낼 일) · 이미 아는 요청 · 끝나서 지움 · 모양이 맞지 않아 무시. */
export type ApplyOutcome = "new" | "known" | "closed" | "ignored";

const AGENTS: readonly AgentId[] = ["news", "strategy", "risk", "review", "trader", "leader"];
const LEADER: AgentId = "leader";
const PENDING = "pending";
const WAITING = "waiting";
const CLOSED_STATES = new Set(["answered", "expired"]);

function agentOf(value: unknown): AgentId {
  return typeof value === "string" && (AGENTS as readonly string[]).includes(value) ? (value as AgentId) : LEADER;
}

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** 이벤트가 기다림인가(`state`가 있으면 그것, 없으면 `status`). 모양이 맞지 않으면 `null`. */
function waitingOf(p: Record<string, unknown>): boolean | null {
  if (typeof p.state === "string") {
    if (p.state === WAITING) return true;
    if (CLOSED_STATES.has(p.state)) return false;
  }
  if (typeof p.status === "string") return p.status === PENDING;
  return null;
}

/** 승인 기다림 상태. */
export class ApprovalBubbles {
  private readonly items = new Map<string, WaitingApproval>();

  /** `office.approval` 이벤트 하나를 반영한다(위 규칙). */
  apply(payload: unknown, now: number): ApplyOutcome {
    const p = record(payload);
    if (!p || typeof p.number !== "string" || p.number === "") return "ignored";
    const waiting = waitingOf(p);
    if (waiting === null) return "ignored";
    const number = p.number;
    if (!waiting) {
      this.items.delete(number);
      return "closed";
    }
    const known = this.items.get(number);
    if (known) {
      // 같은 요청: 차례(at)는 그대로 두고 보이는 글만 고친다.
      if (typeof p.summary === "string") known.summary = p.summary;
      if (typeof p.short_code === "string") known.short_code = p.short_code;
      return "known";
    }
    this.items.set(number, {
      number,
      agent: agentOf(p.agent),
      at: now,
      summary: text(p.summary),
      short_code: text(p.short_code),
    });
    return "new";
  }

  /**
   * 다시 읽은 `pending_approvals`로 통째로 맞춘다: 목록에 없는 것은 지우고 있는 것은 넣거나 고친다. 모양이 맞지 않는
   * 줄은 건너뛴다. 배열이 아니면(그 칸이 없으면) 아무것도 바꾸지 않는다. 새로 넣은 것도 "새 요청"으로 치지 않는다.
   */
  sync(list: unknown): void {
    if (!Array.isArray(list)) return;
    const next = new Map<string, WaitingApproval>();
    for (const line of list) {
      const p = record(line);
      if (!p || typeof p.number !== "string" || p.number === "") continue;
      const at = typeof p.created_at === "number" && Number.isFinite(p.created_at) ? p.created_at : null;
      const known = this.items.get(p.number);
      if (at === null && !known) continue;
      next.set(p.number, {
        number: p.number,
        agent: agentOf(p.agent),
        at: at ?? (known as WaitingApproval).at,
        summary: typeof p.summary === "string" ? p.summary : (known?.summary ?? ""),
        short_code: typeof p.short_code === "string" ? p.short_code : (known?.short_code ?? ""),
      });
    }
    this.items.clear();
    for (const [number, item] of next) this.items.set(number, item);
  }

  /** 지금 기다리는 승인 모두(오래된 것부터). */
  waiting(): WaitingApproval[] {
    return [...this.items.values()]
      .sort((a, b) => a.at - b.at || (a.number < b.number ? -1 : a.number > b.number ? 1 : 0))
      .map((item) => ({ ...item }));
  }

  /** 그 담당의 ✋ 말풍선. 기다리는 것이 없으면 `undefined`. 시간으로 사라지지 않는다(`now`와 무관). */
  bubble(agent: AgentId, now: number): HandBubble | undefined {
    void now;
    const mine = this.waiting().filter((item) => item.agent === agent);
    if (mine.length === 0) return undefined;
    const mark = markOf("approval");
    return {
      agent,
      oldest: mine[0],
      count: mine.length,
      text: mine.length > 1 ? `${mark.glyph} ${mine.length}` : mark.glyph,
      color: mark.color,
    };
  }

  /** ✋가 떠 있는 담당 모두. */
  bubbles(now: number): HandBubble[] {
    const out: HandBubble[] = [];
    for (const agent of AGENTS) {
      const hand = this.bubble(agent, now);
      if (hand) out.push(hand);
    }
    return out;
  }

  /**
   * 그 담당의 ✋를 눌렀다: 가장 오래된 요청으로 `open`(승인 카드 열기)을 한 번 부르고 `true`. ✋가 없으면 부르지 않고
   * `false`. 누른 것만으로는 ✋가 사라지지 않는다(답하거나 만료될 때까지 남는다).
   */
  press(agent: AgentId, open: (approval: WaitingApproval) => void): boolean {
    const hand = this.bubble(agent, 0);
    if (!hand) return false;
    open(hand.oldest);
    return true;
  }
}
