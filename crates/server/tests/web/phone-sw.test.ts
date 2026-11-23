// 폰 서비스 워커(server 12차 4번, 옛 notify `phone`): 푸시 본문 JSON의 title · body로 알림을 띄우고,
// 알림을 누르면 폰 화면을 연다. `public/phone/sw.js`를 가짜 self로 돌려 본다.
import { beforeEach, describe, expect, it, vi } from "vitest";

const files = import.meta.glob<string>("../../web/public/phone/sw.js", {
  query: "?raw",
  import: "default",
  eager: true,
});

const SCOPE = "http://localhost/phone/";

type Handler = (event: Record<string, unknown>) => void;

interface FakeClient {
  focus: ReturnType<typeof vi.fn>;
  postMessage: ReturnType<typeof vi.fn>;
}

let handlers: Map<string, Handler>;
let waits: Promise<unknown>[];
let clients: FakeClient[];
let showNotification: ReturnType<typeof vi.fn>;
let openWindow: ReturnType<typeof vi.fn>;

function source(): string {
  const texts = Object.values(files);
  expect(texts.length, "public/phone/sw.js가 있다").toBe(1);
  return texts[0];
}

/** sw.js를 가짜 self 위에서 돌린다(서비스 워커 전역 대신). */
function boot(): void {
  handlers = new Map();
  waits = [];
  clients = [];
  showNotification = vi.fn(async () => undefined);
  openWindow = vi.fn(async () => null);
  const self = {
    addEventListener: (type: string, handler: Handler) => handlers.set(type, handler),
    registration: { scope: SCOPE, showNotification },
    clients: {
      matchAll: vi.fn(async () => clients),
      openWindow,
      claim: vi.fn(async () => undefined),
    },
    skipWaiting: vi.fn(async () => undefined),
  };
  new Function("self", source())(self);
}

async function fire(type: string, event: Record<string, unknown>): Promise<void> {
  const handler = handlers.get(type);
  expect(handler, `서비스 워커가 ${type}를 받는다`).toBeTruthy();
  (handler as Handler)({ ...event, waitUntil: (p: Promise<unknown>) => waits.push(p) });
  await Promise.all(waits);
}

const pushOf = (payload: unknown) => ({
  data: { json: () => payload, text: () => JSON.stringify(payload) },
});

beforeEach(() => boot());

describe("폰 서비스 워커", () => {
  it("푸시 본문 JSON의 title · body로 알림을 띄운다", async () => {
    // 검증: V10, V11
    await fire("push", pushOf({ title: "승인이 필요해요", body: "삼성전자 10주 사기", kind: "approval_request", seq: 3 }));
    expect(showNotification).toHaveBeenCalledTimes(1);
    const [title, options] = showNotification.mock.calls[0] as [string, { body?: string }];
    expect(title).toBe("승인이 필요해요");
    expect(options.body).toBe("삼성전자 10주 사기");
  });

  it("알림을 누르면 열린 창이 없을 때 폰 화면을 새로 연다", async () => {
    // 검증: V10, V11
    const close = vi.fn();
    await fire("notificationclick", { notification: { close, data: { link: "./" } } });
    expect(close).toHaveBeenCalled();
    expect(openWindow).toHaveBeenCalledWith(SCOPE);
  });

  it("알림의 링크(승인 카드)가 있으면 그 폰 화면 주소를 연다", async () => {
    // 검증: V10, V11
    await fire("notificationclick", { notification: { close: vi.fn(), data: { link: "/phone/approvals/7" } } });
    expect(openWindow).toHaveBeenCalledWith("http://localhost/phone/approvals/7");
  });

  it("이미 열린 폰 화면이 있으면 그 창을 앞으로 가져온다", async () => {
    // 검증: V10, V11
    const client: FakeClient = { focus: vi.fn(async () => undefined), postMessage: vi.fn() };
    clients.push(client);
    await fire("notificationclick", { notification: { close: vi.fn(), data: { link: "./" } } });
    expect(client.focus).toHaveBeenCalled();
    expect(openWindow).not.toHaveBeenCalled();
  });
});
