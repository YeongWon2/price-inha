// 내 앞으로 모이기(결정 29): 팀 방에서 멘션하면 멘션된 담당 모두가 곧바로 오는 중이 되고, 담당 탭을 눌러 말을 걸면
// 대화가 곧바로 열리고 그 담당이 내 곁으로 와서 도착을 알린다. 시연 서버(가짜 AI). 고정 대기 대신 상태를 기다린다.
// 전략 담당은 office.spec.ts의 부르기 시험이 쓰므로 고르지 않는다.
import type { Page } from "@playwright/test";
import { ensureTemplate, expect, openOffice, test } from "./support/fixtures";

const NAMES: Record<string, string> = { news: "뉴스 공시 시세", risk: "리스크", review: "복기", trader: "매매", leader: "팀장" };

interface AgentRow {
  agent: string;
  place: string;
  phase: string;
  reason: string | null;
}

async function agents(page: Page): Promise<AgentRow[]> {
  const res = await page.request.get("/api/office");
  expect(res.ok(), `GET /api/office → ${res.status()}`).toBe(true);
  return ((await res.json()) as { agents: AgentRow[] }).agents;
}

/** 자리에 있고 논의 중이 아닌 담당(전략 빼고) 가운데 앞에서 `n`명. */
async function free(page: Page, n: number): Promise<string[]> {
  const rows = await agents(page);
  const ids = rows.filter((r) => r.agent in NAMES && r.phase === "idle" && r.place !== "meeting").map((r) => r.agent);
  expect(ids.length, `자리에 있는 담당이 ${n}명 이상이어야 한다: ${JSON.stringify(rows)}`).toBeGreaterThanOrEqual(n);
  return ids.slice(0, n);
}

/** 화면과 같은 출처 · 세션으로 JSON을 보낸다(보내기 간격에 걸리면 다시 보낸다). */
async function postJson(page: Page, url: string, body: unknown): Promise<number> {
  let status = 0;
  await expect
    .poll(async () => {
      status = await page.evaluate(
        async ([u, b]) => (await fetch(u, { method: "POST", headers: { "Content-Type": "application/json" }, body: b })).status,
        [url, JSON.stringify(body)] as const,
      );
      return status;
    }, { timeout: 10_000 })
    .not.toBe(429);
  return status;
}

test("팀 방에서 둘을 멘션해 보내면 둘 다 곧바로 내 곁으로 오는 중이 된다", async ({ page }) => {
  // 화면이 세션을 얻을 때까지 기다린다(바로 부르면 세션 전이라 401이 날 수 있다).
  await openOffice(page);
  const [a, b] = await free(page, 2);
  const text = `@[${NAMES[a]}] @[${NAMES[b]}] 모여 주세요`;
  expect(await postJson(page, "/api/office/chat", { room: "team", text, sourceMessageId: "e2e-gather-1" })).toBe(200);
  // 글을 받은 응답이 돌아온 그때 둘 다 이미 불려 있다(첫 담당의 답을 기다리지 않는다).
  const rows = await agents(page);
  for (const id of [a, b]) {
    const row = rows.find((r) => r.agent === id);
    expect(["called", "waiting"], `${id}: ${JSON.stringify(row)}`).toContain(row?.phase);
    expect(row?.reason).toBe("mention");
  }
  // 팀 방을 떠나면 돌아간다(다음 시험을 위해 치운다).
  expect(await postJson(page, "/api/office/summon/view", { room: "direct" })).toBe(200);
});

test("담당 탭을 눌러 말을 걸면 대화가 곧바로 열리고 그 담당이 내 곁으로 온다", async ({ page }) => {
  await ensureTemplate(page);
  await openOffice(page);
  const [id] = await free(page, 1);
  const summoned = page.waitForRequest(
    (r) => r.url().endsWith("/api/office/summon") && r.postDataJSON()?.agent === id,
    { timeout: 40_000 },
  );
  const arrived = page.waitForRequest(
    (r) => r.url().endsWith("/api/office/summon/arrived") && r.postDataJSON()?.agent === id,
    { timeout: 40_000 },
  );
  await page.locator('#chat-tabs [role="tab"]', { hasText: new RegExp(`^${NAMES[id]}`) }).click();
  // 대화는 걸어오기를 기다리지 않고 곧바로 열린다(도착 알림보다 먼저 고른 탭이 된다).
  await expect(page.locator('#chat-tabs [role="tab"][aria-selected="true"]')).toHaveText(new RegExp(`^${NAMES[id]}`), {
    timeout: 10_000,
  });
  await summoned;
  await arrived;
  await expect.poll(async () => (await agents(page)).find((r) => r.agent === id)?.phase, { timeout: 10_000 }).toBe("waiting");
  const row = (await agents(page)).find((r) => r.agent === id);
  expect(row?.reason).toBe("call");
});
