// 폰 화면(server 12차 4번, 옛 notify `phone`): 화면 요소 · 승인 카드 모양 · 하루 정리 구역을 HTML에서 보고,
// `src/phone/main.ts`를 jsdom에서 돌려 푸시 구독과 결정 4번(지문을 그대로 보냄)을 본다.
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { API, approvalPath, decidePath } from "../../web/src/paths";

const pages = import.meta.glob<string>("../../web/phone/index.html", { query: "?raw", import: "default", eager: true });
const sources = import.meta.glob<string>("../../web/src/phone/main.ts", { query: "?raw", import: "default", eager: true });

const ID = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const FULL = "a1b2c3" + "d".repeat(58);

function only(files: Record<string, string>, what: string): string {
  const texts = Object.values(files);
  expect(texts.length, `${what}가 있다`).toBe(1);
  return texts[0];
}
const html = () => only(pages, "phone/index.html");
const appSource = () => only(sources, "src/phone/main.ts");

/** 화면 HTML을 문서로 읽는다(스크립트는 돌리지 않는다). */
function parsed(): Document {
  return new DOMParser().parseFromString(html(), "text/html");
}

describe("폰 화면 HTML", () => {
  it("알림 목록 · 3D 사무실 단추 · viewport 메타 · manifest 연결이 있다", () => {
    // 검증: V10
    const doc = parsed();
    expect(doc.getElementById("notice-list")).not.toBeNull();
    const office = doc.querySelector<HTMLElement>('[data-action="open-office"]');
    expect(office).not.toBeNull();
    expect(office?.textContent).toContain("3D 사무실");
    expect(doc.querySelector('meta[name="viewport"]')).not.toBeNull();
    expect(doc.querySelector('link[rel="manifest"]')?.getAttribute("href")).toContain("manifest.webmanifest");
  });

  it("승인 카드에 승인 · 거절 단추, 6자리 코드 칸, 승인 번호 · 지문 자리가 있다", () => {
    // 검증: V10
    const card = parsed().querySelector<HTMLElement>('[data-card="approval"]');
    expect(card).not.toBeNull();
    const c = card as HTMLElement;
    expect(c.querySelector('[data-action="approve"]')).not.toBeNull();
    expect(c.querySelector('[data-action="reject"]')).not.toBeNull();
    expect(c.hasAttribute("data-approval-id")).toBe(true);
    expect(c.hasAttribute("data-fingerprint")).toBe(true);
    const code = c.querySelector<HTMLInputElement>('input[data-field="code"]');
    expect(code).not.toBeNull();
    expect(code?.getAttribute("maxlength")).toBe("6");
    expect(code?.getAttribute("inputmode")).toBe("numeric");
  });

  it("코드 칸은 보이기만 한다(결정 4번: readonly, one-time-code · pattern 없음)", () => {
    // 검증: V10
    const code = parsed().querySelector<HTMLInputElement>('input[data-field="code"]') as HTMLInputElement;
    expect(code.hasAttribute("readonly")).toBe(true);
    expect(code.getAttribute("autocomplete") ?? "").not.toContain("one-time-code");
    expect(code.hasAttribute("pattern")).toBe(false);
  });

  it("하루 정리는 한 구역 안에 한 일 · 손익 · 복기가 함께 있다", () => {
    // 검증: V12
    const section = parsed().getElementById("daily-summary");
    expect(section).not.toBeNull();
    for (const part of ["done", "pnl", "review"]) {
      expect(section?.querySelector(`[data-part="${part}"]`), `하루 정리 구역에 ${part}`).not.toBeNull();
    }
  });

  it("앱 소스는 6자리 숫자 입력을 강제하지 않고 코드 칸에 친 글자를 읽지 않는다(결정 4번)", () => {
    // 검증: V10
    const src = appSource();
    expect(src.includes("[0-9]{6}")).toBe(false);
    expect(src.includes("\\d{6}")).toBe(false);
    expect(src.includes(".value.trim()")).toBe(false);
  });
});

// ---------- main.ts를 jsdom에서 돌린다 ----------

const KEY_TEXT = "AQID"; // base64url → [1, 2, 3]

interface Call {
  url: string;
  method: string;
  body: unknown;
}
const calls: Call[] = [];

const subscribe = vi.fn(async (_options: PushSubscriptionOptionsInit) => ({
  toJSON: () => ({ endpoint: "http://127.0.0.1/push/1", keys: { p256dh: "p", auth: "a" } }),
}));
const registration = { pushManager: { subscribe } };
const worker = Object.assign(new EventTarget(), {
  register: vi.fn(async (_url: string, _options?: RegistrationOptions) => registration),
});
const registered: unknown[][] = [];

function reply(status: number, body: unknown): Response {
  return new Response(body === null ? null : JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

async function fakeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = String(input);
  const method = (init?.method ?? "GET").toUpperCase();
  const body = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
  calls.push({ url, method, body });
  if (url === API.pushKey) return reply(200, { key: KEY_TEXT });
  if (url === API.pushSubscribe) return reply(204, null);
  if (url.startsWith(API.notices)) return reply(200, []);
  if (url === API.dailySummary) return reply(200, { done: "매수 1건", pnl: "+1,000원", review: "잘했다" });
  if (method === "GET" && url === approvalPath(ID)) {
    return reply(200, { number: ID, summary: "삼성전자 10주 사기", fingerprint: FULL, status: "pending" });
  }
  if (method === "POST") return reply(200, { status: "approved" });
  return reply(404, {});
}

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

const card = () => document.querySelector<HTMLElement>('[data-card="approval"]') as HTMLElement;
const click = (selector: string) => (document.querySelector(selector) as HTMLElement).click();

beforeAll(async () => {
  document.body.innerHTML = parsed().body.innerHTML;
  Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: worker });
  vi.stubGlobal("Notification", { requestPermission: vi.fn(async () => "granted") });
  vi.stubGlobal("fetch", vi.fn(fakeFetch));
  await import("../../web/src/phone/main");
  await settle();
  // 시험마다 가짜 함수의 부른 기록이 비워지므로, 처음 띄울 때의 등록은 여기서 적어 둔다.
  registered.push(...worker.register.mock.calls);
});

beforeEach(() => {
  calls.length = 0;
});

describe("폰 앱(jsdom)", () => {
  it("서비스 워커를 폰 범위로 등록한다", () => {
    // 검증: V10, V11
    expect(registered).toContainEqual(["sw.js", { scope: "./" }]);
  });

  it("하루 정리를 받아 그 구역의 한 일 · 손익 · 복기에 채운다", () => {
    // 검증: V12
    const section = document.getElementById("daily-summary") as HTMLElement;
    expect(section.querySelector('[data-part="done"]')?.textContent).toContain("매수 1건");
    expect(section.querySelector('[data-part="pnl"]')?.textContent).toContain("+1,000원");
    expect(section.querySelector('[data-part="review"]')?.textContent).toContain("잘했다");
  });

  it("알림 켜기를 누르면 서버 키로 푸시를 구독하고(userVisibleOnly) 구독을 서버에 보낸다", async () => {
    // 검증: V10, V11
    click('[data-action="enable-push"]');
    await settle();
    expect(subscribe).toHaveBeenCalledTimes(1);
    const options = subscribe.mock.calls[0][0];
    expect(options.userVisibleOnly).toBe(true);
    expect([...(options.applicationServerKey as Uint8Array)]).toEqual([1, 2, 3]);
    const sent = calls.find((c) => c.url === API.pushSubscribe && c.method === "POST");
    expect(sent?.body).toMatchObject({ endpoint: "http://127.0.0.1/push/1", p256dh: "p", auth: "a" });
  });

  it("승인 카드는 지문 앞 6자리를 보이고, 칸 값과 상관없이 카드의 지문을 그대로 보낸다(결정 4번)", async () => {
    // 검증: V10
    worker.dispatchEvent(
      new MessageEvent("message", { data: { type: "open", link: `http://localhost/phone/approvals/${ID}` } }),
    );
    await settle();
    const c = card();
    expect(c.hidden).toBe(false);
    expect(c.dataset.approvalId).toBe(ID);
    expect(c.dataset.fingerprint).toBe(FULL);
    const code = c.querySelector<HTMLInputElement>('input[data-field="code"]') as HTMLInputElement;
    expect(code.value).toBe(FULL.slice(0, 6));

    code.value = "000000"; // 칸에 무엇이 있든 읽지 않는다.
    calls.length = 0;
    click('[data-action="approve"]');
    await settle();
    const posted = calls.filter((x) => x.method === "POST");
    expect(posted).toEqual([{ url: decidePath(ID, "approve"), method: "POST", body: { fingerprint: FULL } }]);
  });

  it("거절도 같은 카드의 지문을 그대로 보낸다", async () => {
    // 검증: V10
    // 앞 시험에서 승인이 성공한 카드는 막힌 채다(15차 2번): 승인 열기 경로로 카드를 새로 띄운 뒤 누른다.
    worker.dispatchEvent(
      new MessageEvent("message", { data: { type: "open", link: `http://localhost/phone/approvals/${ID}` } }),
    );
    await settle();
    calls.length = 0;
    click('[data-action="reject"]');
    await settle();
    const posted = calls.filter((x) => x.method === "POST");
    expect(posted).toEqual([{ url: decidePath(ID, "reject"), method: "POST", body: { fingerprint: FULL } }]);
  });
});
