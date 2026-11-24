// 사무실 지도(server 12차 2번, 20차 고침): 침실 구역은 없다. 여섯 담당 모두 업무 자리와 쉼 자리(라운지 · 탕비실)가 있고 겹치지 않고,
// 팀장 업무 자리는 대표실 안, 논의 탁자 자리는 미팅룸 안에 6개 이상이고, 방 이름표(대표실 · 미팅룸 · 탕비실 또는 라운지)가 있다.
// 지도 모양: agents[id] = { desk, rest }, rooms[] = { id, label, x, y, w, h }(칸 단위).
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

interface Room {
  id: string;
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 지도의 방 목록(이름표 label이 있다). */
function rooms(): Room[] {
  const list = map().rooms;
  expect(Array.isArray(list), "방 목록(rooms)").toBe(true);
  return (list as Room[]).map((r) => {
    for (const k of ["x", "y", "w", "h"] as const) expect(Number.isInteger(r[k]), `${r.id} ${k}`).toBe(true);
    expect(typeof r.label === "string" && r.label.trim() !== "", `${r.id} 이름표`).toBe(true);
    return r;
  });
}

function inside(p: string, r: Room): boolean {
  const [x, y] = p.split(",").map(Number);
  return x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
}

function labeled(list: Room[], word: string): Room[] {
  return list.filter((r) => r.label.includes(word));
}

describe("사무실 지도", () => {
  it("여섯 명 모두 업무 자리와 쉼 자리가 있고, 서로 겹치지 않고, 침대 · 침실이 없다", () => {
    // 검증: V1, V15
    const m = map();
    const agents = m.agents as Record<string, Record<string, unknown>>;
    expect(typeof agents).toBe("object");
    const seen = new Set<string>();
    for (const id of AGENTS) {
      const a = agents[id];
      expect(a, `${id} 담당이 지도에 있다`).toBeTruthy();
      expect(a.bed, `${id}: 침대 자리는 없다(쉼 자리로 바뀜)`).toBeUndefined();
      const desk = cell(a.desk, `${id} 업무 자리`);
      const rest = cell(a.rest, `${id} 쉼 자리`);
      expect(desk, `${id} 업무 자리와 쉼 자리는 다른 곳`).not.toBe(rest);
      for (const [what, p] of [["업무 자리", desk], ["쉼 자리", rest]]) {
        expect(seen.has(p), `자리가 겹친다: ${id} ${what} ${p}`).toBe(false);
        seen.add(p);
      }
    }
    expect(Object.keys(agents).sort(), "모르는 담당이 없고 정확히 여섯").toEqual([...AGENTS].sort());
    const furniture = (m.furniture ?? []) as { kind?: string }[];
    expect(furniture.some((f) => f.kind === "bed"), "침대 가구가 없다").toBe(false);
    expect(rooms().some((r) => /침실|담당 방/.test(r.label)), "침실 구역이 없다").toBe(false);
  });

  it("방 이름표에 대표실 · 미팅룸 · 탕비실(또는 라운지)이 있고 방 칸은 서로 겹치지 않는다", () => {
    // 검증: V15
    const list = rooms();
    expect(labeled(list, "대표실").length).toBe(1);
    expect(labeled(list, "미팅룸").length).toBe(1);
    expect(labeled(list, "탕비실").length + labeled(list, "라운지").length).toBeGreaterThanOrEqual(1);
    const ids = new Set(list.map((r) => r.id));
    expect(ids.size, "방 id는 서로 다르다").toBe(list.length);
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        const a = list[i];
        const b = list[j];
        const apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        expect(apart, `방이 겹친다: ${a.label} · ${b.label}`).toBe(true);
      }
    }
  });

  it("팀장 업무 자리는 대표실 안이고 모든 쉼 자리는 탕비실 · 라운지 안이다", () => {
    // 검증: V15
    const agents = map().agents as Record<string, Record<string, unknown>>;
    const list = rooms();
    const [ceo] = labeled(list, "대표실");
    expect(ceo, "대표실").toBeTruthy();
    expect(inside(cell(agents.leader.desk, "팀장 업무 자리"), ceo), "팀장 업무 자리는 대표실 안").toBe(true);
    const rests = [...labeled(list, "탕비실"), ...labeled(list, "라운지")];
    for (const id of AGENTS) {
      const p = cell(agents[id].rest, `${id} 쉼 자리`);
      expect(rests.some((r) => inside(p, r)), `${id} 쉼 자리는 탕비실 · 라운지 안: ${p}`).toBe(true);
    }
  });

  it("논의 탁자 자리는 6개 이상이고 모두 미팅룸 안이며 서로 · 업무 자리 · 쉼 자리와 겹치지 않는다", () => {
    // 검증: V1, V15
    const m = map();
    const agents = m.agents as Record<string, Record<string, unknown>>;
    const taken = new Set<string>();
    for (const id of AGENTS) {
      taken.add(cell(agents[id].desk, `${id} 업무 자리`));
      taken.add(cell(agents[id].rest, `${id} 쉼 자리`));
    }
    const [meeting] = labeled(rooms(), "미팅룸");
    expect(meeting, "미팅룸").toBeTruthy();
    const seats = m.meeting_seats;
    expect(Array.isArray(seats), "논의 탁자 자리 목록").toBe(true);
    const list = seats as unknown[];
    expect(list.length, "논의 탁자에 6명이 앉는다").toBeGreaterThanOrEqual(6);
    const seen = new Set<string>();
    list.forEach((seat, i) => {
      const p = cell(seat, `논의 자리 ${i}`);
      expect(seen.has(p), `논의 자리가 겹친다: ${p}`).toBe(false);
      seen.add(p);
      expect(taken.has(p), `논의 자리는 업무 · 쉼 자리와 다른 곳: ${p}`).toBe(false);
      expect(inside(p, meeting), `논의 자리는 미팅룸 안: ${p}`).toBe(true);
    });
  });
});
