// 내 캐릭터 읽기 · 자리 남기기(server 32차 1번 · 2번 · 8번, C17 · M8 · E59 · V56): 걷는 동안 매 칸 보내지 않고 1초에
// 많아야 한 번, 멈춘 뒤 마지막 자리를 1초 안에, 화면을 떠날 때 한 번 보낸다. 실패해도 던지지 않는다. 시각은 시험이 넘긴다
// (가짜 시계). 서버는 가짜 fetch다(바깥 호출 없음).
import { describe, expect, it } from "vitest";
import { API } from "../../web/src/paths";
import {
  SAVE_INTERVAL_MS,
  SpotSender,
  loadMe,
  postSpot,
  type Fetcher,
  type Spot,
} from "../../web/src/office/player-sync";

interface Call {
  url: string;
  init?: RequestInit;
}

function fakeFetch(answer: () => Promise<Response>): { fetcher: Fetcher; calls: Call[] } {
  const calls: Call[] = [];
  const fetcher: Fetcher = (url, init) => {
    calls.push({ url, init });
    return answer();
  };
  return { fetcher, calls };
}

const json = (status: number, body: unknown) =>
  Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

/** 보낸 것(시각 · 자리)을 적는 가짜 보내기. */
function recorder(fail: "no" | "reject" | "throw" = "no") {
  const sent: { at: number; spot: Spot }[] = [];
  let now = 0;
  const send = (spot: Spot): Promise<unknown> => {
    sent.push({ at: now, spot });
    if (fail === "throw") throw new Error("보내기 실패");
    if (fail === "reject") return Promise.reject(new Error("보내기 실패"));
    return Promise.resolve(true);
  };
  return { sent, send, setNow: (t: number) => (now = t) };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("내 캐릭터 읽기", () => {
  it("서버의 이름 · 외형 · 자리를 읽는다", async () => {
    // 검증: V56 C17
    const me = { name: "나", look: "office-jun", x: 37, y: 28, dir: "up" };
    const { fetcher, calls } = fakeFetch(() => json(200, me));
    expect(await loadMe(fetcher)).toEqual(me);
    expect(calls.map((c) => c.url)).toEqual([API.officeMe]);
    expect(API.officeMe).toBe("/api/office/me");
  });

  it("실패하거나 모양이 틀리면 null이고 던지지 않는다", async () => {
    // 검증: V56 C17
    expect(await loadMe(fakeFetch(() => json(401, { error: "unauthorized" })).fetcher)).toBeNull();
    expect(await loadMe(fakeFetch(() => Promise.reject(new Error("끊김"))).fetcher)).toBeNull();
    expect(await loadMe(fakeFetch(() => json(200, { name: "나", look: "office-jun", y: 1, dir: "up" })).fetcher)).toBeNull();
    expect(await loadMe(fakeFetch(() => json(200, { name: "나", look: "office-jun", x: 1, y: 1, dir: "north" })).fetcher)).toBeNull();
  });
});

describe("자리 보내기", () => {
  it("POST /api/office/me/position에 {x, y, dir} JSON을 보낸다", async () => {
    // 검증: V56 C17
    const { fetcher, calls } = fakeFetch(() => json(200, {}));
    expect(await postSpot(fetcher, { x: 5, y: 6, dir: "left" })).toBe(true);
    expect(API.officeMePosition).toBe("/api/office/me/position");
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe(API.officeMePosition);
    expect(calls[0].init?.method).toBe("POST");
    expect(JSON.parse(String(calls[0].init?.body))).toEqual({ x: 5, y: 6, dir: "left" });
  });

  it("거부 · 끊김이면 false이고 던지지 않는다", async () => {
    // 검증: V56 C17
    expect(await postSpot(fakeFetch(() => json(400, { error: "invalid_input" })).fetcher, { x: 1, y: 1, dir: "up" })).toBe(false);
    expect(await postSpot(fakeFetch(() => Promise.reject(new Error("끊김"))).fetcher, { x: 1, y: 1, dir: "up" })).toBe(false);
  });
});

describe("띄엄띄엄 남기기", () => {
  it("걷는 동안 매 칸 보내지 않고 1초에 많아야 한 번 보낸다", () => {
    // 검증: V56 M8
    expect(SAVE_INTERVAL_MS).toBe(1000);
    const r = recorder();
    const sender = new SpotSender(r.send);
    for (let t = 0; t <= 2950; t += 50) {
      r.setNow(t);
      sender.moved({ x: 5 + Math.floor(t / 250), y: 5, dir: "right" }, t);
      sender.tick(t);
    }
    expect(r.sent.length).toBeGreaterThanOrEqual(1);
    expect(r.sent.length).toBeLessThanOrEqual(3);
    for (let i = 1; i < r.sent.length; i += 1) {
      expect(r.sent[i].at - r.sent[i - 1].at).toBeGreaterThanOrEqual(SAVE_INTERVAL_MS);
    }
  });

  it("멈춘 뒤 1초 안에 마지막 자리를 한 번 보내고, 같은 자리는 다시 보내지 않는다", () => {
    // 검증: V56 M8
    const r = recorder();
    const sender = new SpotSender(r.send);
    for (let t = 0; t <= 2950; t += 50) {
      r.setNow(t);
      sender.moved({ x: 5 + Math.floor(t / 250), y: 5, dir: "right" }, t);
      sender.tick(t);
    }
    const last: Spot = { x: 16, y: 5, dir: "right" };
    for (let t = 3000; t <= 3950; t += 50) {
      r.setNow(t);
      sender.moved(last, t);
      sender.tick(t);
    }
    expect(r.sent[r.sent.length - 1].spot).toEqual(last);
    const count = r.sent.length;
    for (let t = 4000; t <= 9000; t += 50) {
      r.setNow(t);
      sender.moved(last, t);
      sender.tick(t);
    }
    expect(r.sent.length).toBe(count);
  });

  it("방향만 바뀌어도 새 자리다", () => {
    // 검증: V56 M8
    const r = recorder();
    const sender = new SpotSender(r.send, { x: 3, y: 3, dir: "down" });
    sender.moved({ x: 3, y: 3, dir: "left" }, 5_000);
    sender.tick(6_000);
    expect(r.sent.map((s) => s.spot)).toEqual([{ x: 3, y: 3, dir: "left" }]);
  });

  it("서버에서 읽은 처음 자리는 보낸 것으로 본다", () => {
    // 검증: V56 M8
    const r = recorder();
    const sender = new SpotSender(r.send, { x: 37, y: 28, dir: "up" });
    sender.moved({ x: 37, y: 28, dir: "up" }, 0);
    sender.tick(5_000);
    sender.flush();
    expect(r.sent).toEqual([]);
  });

  it("화면을 떠날 때 보내지 않은 자리를 곧바로 보내고, 남은 게 없으면 보내지 않는다", () => {
    // 검증: V56 M8
    const r = recorder();
    const sender = new SpotSender(r.send);
    r.setNow(0);
    sender.moved({ x: 1, y: 1, dir: "right" }, 0);
    r.setNow(100);
    sender.moved({ x: 2, y: 1, dir: "right" }, 100);
    const before = r.sent.length;
    sender.flush();
    expect(r.sent.length).toBe(before + 1);
    expect(r.sent[r.sent.length - 1].spot).toEqual({ x: 2, y: 1, dir: "right" });
    sender.flush();
    expect(r.sent.length).toBe(before + 1);
  });

  it("보내기가 실패해도 던지지 않고 다음 자리도 보낸다", async () => {
    // 검증: V56 M8
    for (const fail of ["reject", "throw"] as const) {
      const r = recorder(fail);
      const sender = new SpotSender(r.send);
      expect(() => {
        sender.moved({ x: 1, y: 1, dir: "up" }, 0);
        sender.tick(0);
        sender.moved({ x: 1, y: 2, dir: "down" }, 1_500);
        sender.tick(2_600);
        sender.moved({ x: 1, y: 3, dir: "down" }, 2_700);
        sender.flush();
      }, fail).not.toThrow();
      await settle();
      expect(r.sent.length, fail).toBeGreaterThanOrEqual(2);
      expect(r.sent[r.sent.length - 1].spot, fail).toEqual({ x: 1, y: 3, dir: "down" });
    }
  });
});
