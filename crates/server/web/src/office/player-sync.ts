// 내 캐릭터 읽기 · 자리 남기기(server 32차 1번 · 2번 · 8번, C17 · M8 · E59).
// 읽기: `GET /api/office/me` → `{"name", "look", "x", "y", "dir"}`(실패하면 `null` — 화면은 기본값 · 입구로 선다).
// 남기기는 띄엄띄엄: 걷는 동안 매 칸 보내지 않고 1초에 많아야 한 번, 멈춘 뒤에는 마지막 자리를 1초 안에 한 번,
// 화면을 떠날 때(`flush`) 한 번 보낸다. 보내기가 실패해도 던지지 않는다(걷기는 그대로 — F10 "실패 시 보장").
// 다른 화면에 내 자리를 중계하지 않는다(마지막으로 저장한 쪽이 남는다 — E59). 시각은 부르는 쪽이 넘긴다(가짜 시계).

import { API } from "../paths";
import type { Facing } from "./player";

/** 자리를 남기는 가장 짧은 간격(밀리초). */
export const SAVE_INTERVAL_MS = 1000;

/** 남길 자리: 칸(정수)과 바라보는 방향. */
export interface Spot {
  x: number;
  y: number;
  dir: Facing;
}

/** 서버가 주는 내 캐릭터. */
export interface Me extends Spot {
  name: string;
  look: string;
}

/** 같은 출처 `fetch`의 모양(시험이 가짜를 넘긴다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

const FACINGS: readonly Facing[] = ["up", "down", "left", "right"];

function isCell(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

/** 내 캐릭터를 읽는다([`API.officeMe`]). 응답이 실패이거나 모양이 틀리면 `null`(던지지 않는다). */
export async function loadMe(fetcher: Fetcher): Promise<Me | null> {
  try {
    const reply = await fetcher(API.officeMe, { credentials: "same-origin" });
    if (!reply.ok) return null;
    const body = (await reply.json()) as Record<string, unknown> | null;
    if (!body || typeof body !== "object") return null;
    const { name, look, x, y, dir } = body;
    if (typeof name !== "string" || typeof look !== "string") return null;
    if (!isCell(x) || !isCell(y)) return null;
    if (typeof dir !== "string" || !(FACINGS as readonly string[]).includes(dir)) return null;
    return { name, look, x, y, dir: dir as Facing };
  } catch {
    return null;
  }
}

/**
 * 자리 하나를 보낸다: `POST` [`API.officeMePosition`] 몸통 `{"x", "y", "dir"}`(JSON, 같은 출처 · 떠날 때도 닿게
 * `keepalive`). 실패해도 던지지 않고 `false`를 돌려준다.
 */
export async function postSpot(fetcher: Fetcher, spot: Spot): Promise<boolean> {
  try {
    const reply = await fetcher(API.officeMePosition, {
      method: "POST",
      credentials: "same-origin",
      keepalive: true,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ x: spot.x, y: spot.y, dir: spot.dir }),
    });
    return reply.ok;
  } catch {
    return false;
  }
}

function same(a: Spot | null, b: Spot | null): boolean {
  return a !== null && b !== null && a.x === b.x && a.y === b.y && a.dir === b.dir;
}

/**
 * 띄엄띄엄 보내는 것. `send`는 자리를 보내는 함수(보통 `postSpot`)이고 실패(거부 · 던짐)해도 이것은 던지지 않는다.
 * - `moved(spot, now)`: 지금 자리. 마지막으로 보낸 자리와 다르고 마지막으로 보낸 지 [`SAVE_INTERVAL_MS`]가 지났으면 보낸다.
 * - `tick(now)`: 프레임마다 부른다. 보내지 않은 자리가 있고 간격이 지났으면 보낸다(멈춘 뒤 마지막 자리).
 * - `flush()`: 화면을 떠날 때. 보내지 않은 자리가 있으면 간격과 상관없이 곧바로 보낸다.
 * 같은 자리(칸 · 방향이 같음)는 다시 보내지 않는다. 처음 만들 때 `initial`(서버에서 읽은 자리)을 보낸 것으로 본다.
 */
export class SpotSender {
  private readonly send: (spot: Spot) => Promise<unknown>;
  private sent: Spot | null;
  private sentAt = -Infinity;
  private pending: Spot | null = null;

  constructor(send: (spot: Spot) => Promise<unknown>, initial: Spot | null = null) {
    this.send = send;
    this.sent = initial ? { ...initial } : null;
  }

  moved(spot: Spot, now: number): void {
    this.pending = same(spot, this.sent) ? null : { x: spot.x, y: spot.y, dir: spot.dir };
    this.tick(now);
  }

  tick(now: number): void {
    if (this.pending === null || now - this.sentAt < SAVE_INTERVAL_MS) return;
    this.sentAt = now;
    this.push();
  }

  flush(): void {
    if (this.pending === null) return;
    this.push();
  }

  private push(): void {
    const spot = this.pending;
    if (spot === null) return;
    this.pending = null;
    this.sent = spot;
    try {
      void Promise.resolve(this.send(spot)).catch(() => undefined);
    } catch {
      // 보내기가 실패해도 걷기는 그대로다(F10 "실패 시 보장").
    }
  }
}
