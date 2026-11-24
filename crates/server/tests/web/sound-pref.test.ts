// 알림음 켜기 · 끄기(server 31차 1번 (바), 31차 고침 · 결정 22 · V78): 서버(`API.setupSound`)에서 읽고 서버에 쓴다.
// 기본은 켜짐이고, 읽기 · 쓰기가 실패해도 던지지 않으며(읽기는 켜짐, 쓰기는 `false` · `null`), 브라우저 저장소를
// 건드리지 않는다(V46 · D11 · N8). 쓰기는 같은 출처 부르기(`ui/call.ts`)와 같은 모양이다.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";
import { SOUND_DEFAULT, saveSound, setSoundEnabled, soundEnabled } from "../../web/src/ui/sound-pref";
import { SOURCES, reach } from "./source-graph";

const SOURCE = "../../web/src/ui/sound-pref.ts";

interface Sent {
  url: string;
  path: string;
  init: RequestInit | undefined;
}

/** 서버 답 하나를 만드는 것(던지면 연결 실패). */
type Answer = (sent: Sent) => Response | Promise<Response>;

let sent: Sent[] = [];

/** 가짜 fetch를 단다: 모든 요청을 적고 `answer`로 답한다. */
function serve(answer: Answer): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const one = { url, path: new URL(url, "http://local.test").pathname, init };
      sent.push(one);
      return answer(one);
    }),
  );
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

/** 보낸 몸통(JSON). */
function bodyOf(one: Sent): unknown {
  return typeof one.init?.body === "string" ? JSON.parse(one.init.body) : null;
}

/** 머리 칸 하나(대소문자 무시). */
function headerOf(one: Sent, name: string): string | null {
  return new Headers(one.init?.headers).get(name);
}

/** 브라우저 저장소(웹 저장소 · 쿠키)를 건드리면 적는 감시. */
function watchStorage(): () => number {
  const spies = [
    vi.spyOn(Storage.prototype, "getItem"),
    vi.spyOn(Storage.prototype, "setItem"),
    vi.spyOn(Storage.prototype, "removeItem"),
    vi.spyOn(Storage.prototype, "clear"),
  ];
  const cookie = vi.spyOn(Document.prototype, "cookie", "set");
  return () => spies.reduce((sum, spy) => sum + spy.mock.calls.length, 0) + cookie.mock.calls.length;
}

beforeEach(() => {
  sent = [];
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("알림음 설정 · 읽기(서버)", () => {
  it("서버가 켜짐이라 답하면 켜짐, 꺼짐이라 답하면 꺼짐이고 GET 하나로 읽는다", async () => {
    // 검증: V78 R18-4
    serve(() => json(200, { enabled: false }));
    expect(await soundEnabled()).toBe(false);
    expect(sent.length).toBe(1);
    expect(sent[0].path).toBe(API.setupSound);
    expect((sent[0].init?.method ?? "GET").toUpperCase()).toBe("GET");
    expect(sent[0].url.includes("://"), "같은 출처 경로").toBe(false);
    serve(() => json(200, { enabled: true }));
    expect(await soundEnabled()).toBe(true);
  });

  it("읽기가 실패하면(서버 오류 · 연결 실패 · 모양이 다른 답) 던지지 않고 기본(켜짐)이다", async () => {
    // 검증: V78 E57 R18-4
    expect(SOUND_DEFAULT, "기본은 켜짐").toBe(true);
    serve(() => json(500, { error: "internal" }));
    await expect(soundEnabled()).resolves.toBe(true);
    serve(() => json(401, { error: "unauthorized" }));
    await expect(soundEnabled()).resolves.toBe(true);
    serve(() => {
      throw new TypeError("연결 끊김");
    });
    await expect(soundEnabled()).resolves.toBe(true);
    for (const odd of [{ enabled: "false" }, { enabled: 0 }, {}, null, [false]]) {
      serve(() => json(200, odd));
      await expect(soundEnabled(), JSON.stringify(odd)).resolves.toBe(true);
    }
    serve(() => new Response("JSON 아님", { status: 200 }));
    await expect(soundEnabled()).resolves.toBe(true);
  });
});

describe("알림음 설정 · 쓰기(서버)", () => {
  it("끄면 같은 출처로 POST {enabled:false}를 JSON으로 보내고, 서버가 답한 값을 돌려준다", async () => {
    // 검증: V78 R18-4 D11
    serve((one) => json(200, bodyOf(one)));
    expect(await saveSound(false)).toBe(false);
    expect(sent.length).toBe(1);
    const post = sent[0];
    expect(post.path).toBe(API.setupSound);
    expect(post.url.includes("://"), "같은 출처 경로").toBe(false);
    expect(post.init?.method).toBe("POST");
    expect(post.init?.credentials).toBe("same-origin");
    expect(headerOf(post, "Content-Type")).toBe("application/json");
    expect(bodyOf(post)).toEqual({ enabled: false });
    expect(await saveSound(true)).toBe(true);
    expect(bodyOf(sent[1])).toEqual({ enabled: true });
  });

  it("서버가 보낸 값과 다르게 답하면 서버의 답을 돌려준다", async () => {
    // 검증: V78
    serve(() => json(200, { enabled: true }));
    expect(await saveSound(false)).toBe(true);
  });

  it("남기면 setSoundEnabled는 true다", async () => {
    // 검증: V78 R18-4
    serve((one) => json(200, bodyOf(one)));
    expect(await setSoundEnabled(false)).toBe(true);
    expect(bodyOf(sent[0])).toEqual({ enabled: false });
  });

  it("쓰기가 실패하면(거절 · 서버 오류 · 연결 실패 · 모양이 다른 답) 던지지 않고 null · false다", async () => {
    // 검증: V78 E57
    const failures: Answer[] = [
      () => json(400, { error: "invalid_input" }),
      () => json(401, { error: "unauthorized" }),
      () => json(500, { error: "internal" }),
      () => {
        throw new TypeError("연결 끊김");
      },
      () => json(200, { enabled: "off" }),
      () => new Response("JSON 아님", { status: 200 }),
    ];
    for (const answer of failures) {
      serve(answer);
      await expect(saveSound(false)).resolves.toBeNull();
      await expect(setSoundEnabled(false)).resolves.toBe(false);
    }
  });
});

describe("알림음 설정 · 브라우저에 남기지 않음", () => {
  it("읽고 써도 브라우저 저장소 · 쿠키를 건드리지 않는다", async () => {
    // 검증: V46 N8 D11
    const touched = watchStorage();
    serve((one) => json(200, one.init?.method === "POST" ? bodyOf(one) : { enabled: false }));
    await soundEnabled();
    await saveSound(true);
    await setSoundEnabled(false);
    serve(() => json(500, { error: "internal" }));
    await soundEnabled();
    await setSoundEnabled(true);
    expect(touched(), "브라우저 저장소 · 쿠키에 손대지 않는다").toBe(0);
  });

  it("소스는 같은 출처 부르기(ui/call.ts) · 경로 표에 닿고 브라우저 저장소 글자가 없다", () => {
    // 검증: V46 N8 D11
    const { files, bare } = reach(SOURCE);
    expect(files.has("../../web/src/ui/call.ts"), "같은 출처 부르기를 쓴다").toBe(true);
    expect(files.has("../../web/src/paths.ts"), "경로 표를 쓴다").toBe(true);
    expect([...bare]).toEqual([]);
    for (const banned of ["Storage", "indexedDB", "cookie", "location"]) {
      expect(SOURCES[SOURCE].includes(banned), banned).toBe(false);
    }
  });
});
