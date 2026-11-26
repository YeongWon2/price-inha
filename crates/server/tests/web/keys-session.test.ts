// 키 화면(`/keys`)의 세션(기능 H 2번 · backlog "하네스 36차"): 그 컴퓨터에서 세션 없이 바로 열었으면 설정 화면처럼
// 본문 없이 세션을 청해 얻는다. 세션이 이미 있으면(다른 기기가 접속 키로 들어와 연 경우) 청하지 않는다.
// 다른 기기 주소에서는 접속 키 없이 세션을 청하지 않는다(서버가 틀린 시도로 센다).
// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";
import { ensureKeysSession, isLoopbackHost, type Fetcher } from "../../web/src/keys/session";

interface Call {
  path: string;
  method: string;
  body: unknown;
}

/** 등록 여부 읽기가 `slots` 상태로, 세션 청하기가 `session` 상태로 답하는 가짜. 세션을 얻으면 읽기가 200이 된다. */
function fake(slots: number, session: number): { fetcher: Fetcher; calls: Call[] } {
  const calls: Call[] = [];
  let granted = false;
  const fetcher: Fetcher = (path, init) => {
    const method = (init?.method ?? "GET").toUpperCase();
    calls.push({ path, method, body: init?.body });
    if (path === API.session && method === "POST") {
      granted = session === 200;
      return Promise.resolve(new Response("{}", { status: session }));
    }
    const status = granted ? 200 : slots;
    return Promise.resolve(new Response(JSON.stringify({ slots: [] }), { status }));
  };
  return { fetcher, calls };
}

const pages = import.meta.glob<string>("../../web/keys.html", { query: "?raw", import: "default", eager: true });

const posts = (calls: Call[]) => calls.filter((c) => c.method === "POST");

describe("키 화면의 세션", () => {
  it("그 컴퓨터 주소 이름을 안다", () => {
    for (const host of ["127.0.0.1", "localhost", "[::1]", "::1", "127.0.0.2"]) {
      expect(isLoopbackHost(host), host).toBe(true);
    }
    for (const host of ["192.168.0.5", "pi.example.ts.net", "localhost.example.com", ""]) {
      expect(isLoopbackHost(host), host).toBe(false);
    }
  });

  it("세션이 이미 있으면(등록 여부 읽기가 된다) 세션을 청하지 않는다", async () => {
    const { fetcher, calls } = fake(200, 200);
    expect(await ensureKeysSession(fetcher, "127.0.0.1")).toBe(true);
    expect(posts(calls)).toHaveLength(0);
  });

  it("그 컴퓨터에서 세션 없이 열었으면(401) 본문 없이 세션을 한 번 청해 얻는다", async () => {
    const { fetcher, calls } = fake(401, 200);
    expect(await ensureKeysSession(fetcher, "127.0.0.1")).toBe(true);
    expect(posts(calls)).toEqual([{ path: API.session, method: "POST", body: undefined }]);
    expect(calls[0]).toMatchObject({ path: API.setupSlots, method: "GET" });
  });

  it("다른 기기 주소에서는 401이어도 접속 키 없이 세션을 청하지 않는다", async () => {
    const { fetcher, calls } = fake(401, 200);
    expect(await ensureKeysSession(fetcher, "pi.example.ts.net")).toBe(false);
    expect(posts(calls)).toHaveLength(0);
  });

  it("세션을 얻지 못하거나 서버에 닿지 못하면 거짓이다", async () => {
    const { fetcher } = fake(401, 401);
    expect(await ensureKeysSession(fetcher, "localhost")).toBe(false);
    const broken: Fetcher = () => Promise.reject(new Error("down"));
    expect(await ensureKeysSession(broken, "localhost")).toBe(false);
  });

  it("401이 아닌 실패(서버 오류)면 세션을 청하지 않는다", async () => {
    const { fetcher, calls } = fake(500, 200);
    expect(await ensureKeysSession(fetcher, "127.0.0.1")).toBe(false);
    expect(posts(calls)).toHaveLength(0);
  });

  it("키 화면은 세션을 얻은 뒤 등록 여부를 읽는다(그 컴퓨터에서 세션 없이 바로 연 경우)", async () => {
    const html = Object.values(pages)[0];
    document.body.replaceChildren(...new DOMParser().parseFromString(html, "text/html").body.childNodes);
    const { fetcher, calls } = fake(401, 200);
    vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit) => fetcher(String(input), init));
    expect(isLoopbackHost(location.hostname), "jsdom 주소는 그 컴퓨터").toBe(true);
    await import("../../web/src/keys/main");
    await vi.waitFor(() => {
      const reads = calls.filter((c) => c.path === API.setupSlots && c.method === "GET");
      expect(reads.length).toBeGreaterThanOrEqual(2);
    });
    expect(posts(calls).map((c) => c.path)).toEqual([API.session]);
    const order = calls.map((c) => `${c.method} ${c.path}`);
    expect(order.indexOf(`POST ${API.session}`)).toBeLessThan(order.lastIndexOf(`GET ${API.setupSlots}`));
    vi.unstubAllGlobals();
  });
});
