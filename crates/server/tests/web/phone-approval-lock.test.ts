// 폰 승인 카드 두 번 누름 막기(server 15차 2번): 보내는 동안 승인 · 거절 단추가 막히고 POST는 한 번,
// 실패하면 다시 켜지고, 성공하면 막힌 채 둔다.
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { approvalPath, decidePath } from "../../web/src/paths";

const pages = import.meta.glob<string>("../../web/phone/index.html", { query: "?raw", import: "default", eager: true });
const ID = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const FULL = "a1b2c3" + "d".repeat(58);

let posts = 0;
let release: ((status: number) => void) | null = null;

const worker = Object.assign(new EventTarget(), {
  register: vi.fn(async () => ({ pushManager: {} })),
});

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

async function fakeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = String(input);
  const method = (init?.method ?? "GET").toUpperCase();
  if (method === "GET" && url === approvalPath(ID)) {
    return json(200, { number: ID, summary: "삼성전자 10주 사기", fingerprint: FULL, status: "pending" });
  }
  if (method === "POST" && (url === decidePath(ID, "approve") || url === decidePath(ID, "reject"))) {
    posts += 1;
    const status = await new Promise<number>((resolve) => {
      release = resolve;
    });
    return json(status, { status: "approved" });
  }
  return json(404, {});
}

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

const button = (verb: string) => document.querySelector(`[data-action="${verb}"]`) as HTMLButtonElement;

beforeAll(async () => {
  const html = Object.values(pages)[0];
  document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
  Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: worker });
  vi.stubGlobal("Notification", { requestPermission: vi.fn(async () => "granted") });
  vi.stubGlobal("fetch", vi.fn(fakeFetch));
  await import("../../web/src/phone/main");
  worker.dispatchEvent(
    new MessageEvent("message", { data: { type: "open", link: `http://localhost/phone/approvals/${ID}` } }),
  );
  await settle();
});

beforeEach(() => {
  posts = 0;
  release = null;
});

describe("폰 승인 카드 두 번 누름", () => {
  it("보내는 동안 두 단추가 막히고 빨리 두 번 눌러도 POST는 한 번이다. 실패하면 다시 켜진다", async () => {
    // 검증: V10
    button("approve").click();
    button("approve").click();
    button("reject").click();
    await settle();
    expect(posts).toBe(1);
    expect(button("approve").disabled).toBe(true);
    expect(button("reject").disabled).toBe(true);

    release?.(500);
    await settle();
    expect(button("approve").disabled).toBe(false);
    expect(button("reject").disabled).toBe(false);
  });

  it("다시 눌러 성공하면 단추는 막힌 채 두고 더는 보내지 않는다", async () => {
    // 검증: V10
    button("approve").click();
    await settle();
    expect(posts).toBe(1);
    release?.(200);
    await settle();
    expect(button("approve").disabled).toBe(true);
    expect(button("reject").disabled).toBe(true);

    button("approve").click();
    button("reject").click();
    await settle();
    expect(posts).toBe(1);
  });
});
