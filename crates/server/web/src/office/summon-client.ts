// 담당 부르기 · 도착 알림 · 보는 방 알림의 요청(server 33차 2번 · 4번 · 7번, C20 · E47).
// - 부르기 `POST /api/office/summon` `{"agent"}` → 200 `{"agent", "phase": "called"}` · 모르는 담당 400
//   `{"error": "unknown_agent"}` · 회의 중 409 `{"error": "meeting_reserved"}`.
// - 도착 알림 `POST /api/office/summon/arrived` `{"agent"}`: 걸어온 담당이 곁에 섰을 때, 그리고 돌아가던 담당이
//   자리에 닿았을 때 보낸다(원본 `npc:arrived`).
// - 보는 방 알림 `POST /api/office/summon/view` `{"room": "team" | "direct"}`: 화면이 팀 방 ↔ 1:1 방을 바꿀 때만 보낸다.
// 모두 JSON · 같은 출처이고 던지지 않는다.

import { API } from "../paths";

/** 같은 출처 `fetch`의 모양(시험이 가짜를 넘긴다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

/** 부르기의 결과. */
export type SummonResult =
  | { kind: "called" }
  | { kind: "meeting" }
  | { kind: "unknown" }
  | { kind: "failed" };

/** 보는 방(서버가 아는 이름). */
export type ViewRoom = "team" | "direct";

/** 담당을 부른다. 200이면 `called`, 409 `meeting_reserved`면 `meeting`, 400 `unknown_agent`면 `unknown`, 그 밖은 `failed`. */
export async function requestSummon(fetcher: Fetcher, agent: string): Promise<SummonResult> {
  void fetcher;
  void agent;
  void API;
  throw new Error("todo");
}

/** 도착을 알린다. 200이면 참. */
export async function reportArrived(fetcher: Fetcher, agent: string): Promise<boolean> {
  void fetcher;
  void agent;
  throw new Error("todo");
}

/** 지금 보는 방을 알린다. 200이면 참. */
export async function reportView(fetcher: Fetcher, room: ViewRoom): Promise<boolean> {
  void fetcher;
  void room;
  throw new Error("todo");
}
