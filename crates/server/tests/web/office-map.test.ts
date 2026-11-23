// 사무실 지도(server 12차 2번, 옛 office `web_assets`): 여섯 담당 모두 책상과 침대가 있고 둘은 다른 칸,
// 책상끼리 · 침대끼리 겹치지 않고, 방 이름이 서로 다르고, 모르는 담당이 없고, 논의 탁자 자리가 6개 이상이다.
import { describe, expect, it } from "vitest";

const files = import.meta.glob<string>("../../web/public/office/office-map.json", {
  query: "?raw",
  import: "default",
  eager: true,
});

/** 옛 office `Agent::ALL`의 id(사무실 화면의 담당 목록과 같다). */
const AGENTS = ["news", "strategy", "risk", "review", "trader", "leader"];

interface Point {
  x: number;
  y: number;
}

function map(): Record<string, unknown> {
  const texts = Object.values(files);
  expect(texts.length, "public/office/office-map.json이 있다").toBe(1);
  return JSON.parse(texts[0]) as Record<string, unknown>;
}

/** x · y가 0 이상의 정수 칸 번호인지 보고 "x,y" 글자로 돌려준다. */
function cell(value: unknown, what: string): string {
  const p = (value ?? {}) as Partial<Point>;
  const ok = Number.isInteger(p.x) && Number.isInteger(p.y) && (p.x as number) >= 0 && (p.y as number) >= 0;
  expect(ok, `${what}: x · y가 0 이상의 정수 칸 번호여야 한다: ${JSON.stringify(value)}`).toBe(true);
  return `${p.x},${p.y}`;
}

describe("사무실 지도", () => {
  it("여섯 명 모두 책상과 침대가 있고, 겹치지 않고, 방은 각자다", () => {
    // 검증: V1, V15, V13
    const agents = map().agents as Record<string, Record<string, unknown>>;
    expect(typeof agents).toBe("object");
    const desks = new Set<string>();
    const beds = new Set<string>();
    const rooms = new Set<string>();
    for (const id of AGENTS) {
      const a = agents[id];
      expect(a, `${id} 담당이 지도에 있다`).toBeTruthy();
      const desk = cell(a.desk, `${id} 책상`);
      const bed = cell(a.bed, `${id} 침대`);
      expect(desk, `${id} 책상과 침대는 다른 곳`).not.toBe(bed);
      expect(desks.has(desk), `책상이 겹친다: ${id}`).toBe(false);
      expect(beds.has(bed), `침대가 겹친다: ${id}`).toBe(false);
      desks.add(desk);
      beds.add(bed);
      const room = typeof a.room === "string" ? a.room.trim() : "";
      expect(room, `${id} 방 이름`).not.toBe("");
      expect(rooms.has(room), `방은 각자: ${id}`).toBe(false);
      rooms.add(room);
    }
    expect(Object.keys(agents).sort(), "모르는 담당이 없고 정확히 여섯").toEqual([...AGENTS].sort());
  });

  it("논의 탁자 자리는 6개 이상이고 서로 · 책상 · 침대와 겹치지 않는다", () => {
    // 검증: V1, V15
    const m = map();
    const agents = m.agents as Record<string, Record<string, unknown>>;
    const taken = new Set<string>();
    for (const id of AGENTS) {
      taken.add(cell(agents[id].desk, `${id} 책상`));
      taken.add(cell(agents[id].bed, `${id} 침대`));
    }
    const seats = m.meeting_seats;
    expect(Array.isArray(seats), "논의 탁자 자리 목록").toBe(true);
    const list = seats as unknown[];
    expect(list.length, "논의 탁자에 6명이 앉는다").toBeGreaterThanOrEqual(6);
    const seen = new Set<string>();
    list.forEach((seat, i) => {
      const p = cell(seat, `논의 자리 ${i}`);
      expect(seen.has(p), `논의 자리가 겹친다: ${p}`).toBe(false);
      seen.add(p);
      expect(taken.has(p), `논의 자리는 책상 · 침대와 다른 곳: ${p}`).toBe(false);
    });
  });
});
