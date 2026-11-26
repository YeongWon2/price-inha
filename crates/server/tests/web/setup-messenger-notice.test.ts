// 다듬기 E 7번: 메신저 토큰 지우기(`DELETE /api/messenger/telegram`)가 성공하고 답에 `notice`가 있으면, 다시 그린 메신저
// 묶음의 결과 자리에 그 안내(텔레그램 쪽 토큰은 살아 있으니 BotFather에서 폐기)를 글로 보인다. 없으면 보이지 않는다.
// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { mountMessenger } from "../../web/src/setup/messenger-view";

const NOTICE = "텔레그램 쪽 토큰은 살아 있습니다. BotFather에서 토큰을 폐기하세요.";

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

function telegram(configured: boolean): unknown {
  return {
    editable: true,
    telegram: { configured, bot: configured ? "siheom_bot" : null, state: configured ? "polling" : "off", paired: false, pending: 0, locked_until: null },
  };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 가짜 서버로 메신저 묶음을 띄우고 지우기를 누른다. `removed`는 지우기 답. */
async function removeWith(removed: unknown): Promise<HTMLElement> {
  let configured = true;
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const method = (init?.method ?? "GET").toUpperCase();
      const path = new URL(String(input), "http://local.test").pathname;
      if (method === "GET" && path === "/api/messenger") return Promise.resolve(json(200, telegram(configured)));
      if (method === "DELETE" && path === "/api/messenger/telegram") {
        configured = false;
        return Promise.resolve(json(200, removed));
      }
      return Promise.resolve(json(404, { error: "not_found" }));
    }),
  );
  const root = document.createElement("div");
  document.body.append(root);
  await mountMessenger(root);
  await settle();
  root.querySelector<HTMLButtonElement>('button[data-action="messenger-remove"]')?.click();
  await settle();
  return root;
}

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

describe("토큰 지우기 뒤 안내", () => {
  it("답에 안내가 있으면 다시 그린 묶음의 결과 자리에 글로 보인다", async () => {
    const root = await removeWith({ removed: true, notice: NOTICE });
    const state = root.querySelector('[data-role="messenger-state"]');
    expect(state?.getAttribute("data-state")).toBe("off");
    const notice = root.querySelector('[data-role="messenger-message"] [data-role="messenger-notice"]');
    expect(notice?.textContent).toBe(NOTICE);
  });

  it("옛 답(안내 없음)이면 안내를 보이지 않는다", async () => {
    const root = await removeWith({ removed: true });
    expect(root.querySelector('[data-role="messenger-notice"]')).toBeNull();
  });
});
