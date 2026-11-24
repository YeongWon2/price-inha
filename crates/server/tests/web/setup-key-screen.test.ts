// 설정 화면(`/`)의 키 칸 행동(server 19차 6번, 시험만): 소스 글자가 아니라 jsdom에서 단추를 눌러 제출하면
// 받아들여질 때만 칸이 비고, 거절되면 칸이 그대로 남는다(키 화면과 같은 규칙, `key-screens.test.ts`의 글자 시험 보강).
import { beforeAll, describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";

const pages = import.meta.glob<string>("../../web/index.html", { query: "?raw", import: "default", eager: true });

let accept = false;
const bodies: Array<{ item: string; fields: Record<string, string> }> = [];

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

class QuietSocket {
  onopen: (() => void) | null = null;
  onmessage: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  close(): void {}
}

const input = (id: string) => document.getElementById(id) as HTMLInputElement;
const click = (id: string) => (document.getElementById(id) as HTMLButtonElement).click();

beforeAll(async () => {
  const html = Object.values(pages)[0];
  document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
  vi.stubGlobal("WebSocket", QuietSocket);
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "POST" && String(url) === API.setupKeys) {
        bodies.push(JSON.parse(String(init?.body)));
        return accept
          ? new Response(JSON.stringify({ registered: true }), { status: 200 })
          : new Response(JSON.stringify({ registered: false, error: "key_rejected" }), { status: 400 });
      }
      if (method === "POST" && String(url) === API.session) return new Response("{}", { status: 200 });
      return new Response("{}", { status: 404 });
    }),
  );
  await import("../../web/src/setup/main");
  await settle();
});

describe("설정 화면 키 칸", () => {
  it("AI API 키: 거절되면 칸이 그대로 남고, 받아들여지면 칸이 빈다", async () => {
    // 검증: V46 C3 E4
    const before = bodies.length;
    input("f-api-key").value = "typed-ai-value";
    accept = false;
    click("b-ai-key");
    await settle();
    expect(bodies.length).toBe(before + 1);
    expect(bodies[before]).toEqual({ item: "ai_api_key", fields: { api_key: "typed-ai-value" } });
    expect(input("f-api-key").value).toBe("typed-ai-value");

    accept = true;
    click("b-ai-key");
    await settle();
    expect(bodies.length).toBe(before + 2);
    expect(input("f-api-key").value).toBe("");
    expect((document.getElementById("m-ai-key") as HTMLElement).textContent).toBe("등록됨");
  });

  it("증권사 키 두 칸: 거절되면 두 칸이 그대로, 받아들여지면 두 칸이 빈다", async () => {
    // 검증: V46 C3 E4
    input("f-client-id").value = "typed-id";
    input("f-client-secret").value = "typed-secret";
    accept = false;
    click("b-broker");
    await settle();
    expect(input("f-client-id").value).toBe("typed-id");
    expect(input("f-client-secret").value).toBe("typed-secret");

    accept = true;
    click("b-broker");
    await settle();
    expect(input("f-client-id").value).toBe("");
    expect(input("f-client-secret").value).toBe("");
  });

  it("바깥 키 칸(DART): 거절되면 칸이 그대로, 받아들여지면 칸이 빈다", async () => {
    // 검증: V46 C3 E4
    input("f-slot-dart").value = "typed-slot";
    accept = false;
    click("b-slot-dart");
    await settle();
    expect(input("f-slot-dart").value).toBe("typed-slot");

    accept = true;
    click("b-slot-dart");
    await settle();
    expect(input("f-slot-dart").value).toBe("");
  });
});
