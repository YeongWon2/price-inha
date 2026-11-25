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

/** 승인 기다림 상태. */
export class ApprovalBubbles {
  /** `office.approval` 이벤트 하나를 반영한다(위 규칙). */
  apply(payload: unknown, now: number): ApplyOutcome {
    void payload;
    void now;
    throw new Error("todo");
  }

  /**
   * 다시 읽은 `pending_approvals`로 통째로 맞춘다: 목록에 없는 것은 지우고 있는 것은 넣거나 고친다. 모양이 맞지 않는
   * 줄은 건너뛴다. 배열이 아니면(그 칸이 없으면) 아무것도 바꾸지 않는다. 새로 넣은 것도 "새 요청"으로 치지 않는다.
   */
  sync(list: unknown): void {
    void list;
    throw new Error("todo");
  }

  /** 지금 기다리는 승인 모두(오래된 것부터). */
  waiting(): WaitingApproval[] {
    throw new Error("todo");
  }

  /** 그 담당의 ✋ 말풍선. 기다리는 것이 없으면 `undefined`. 시간으로 사라지지 않는다(`now`와 무관). */
  bubble(agent: AgentId, now: number): HandBubble | undefined {
    void agent;
    void now;
    throw new Error("todo");
  }

  /** ✋가 떠 있는 담당 모두. */
  bubbles(now: number): HandBubble[] {
    void now;
    throw new Error("todo");
  }

  /**
   * 그 담당의 ✋를 눌렀다: 가장 오래된 요청으로 `open`(승인 카드 열기)을 한 번 부르고 `true`. ✋가 없으면 부르지 않고
   * `false`. 누른 것만으로는 ✋가 사라지지 않는다(답하거나 만료될 때까지 남는다).
   */
  press(agent: AgentId, open: (approval: WaitingApproval) => void): boolean {
    void agent;
    void open;
    throw new Error("todo");
  }
}
