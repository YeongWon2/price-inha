// 사무실: 내 캐릭터 · 방향키 · 팀 방 멘션 · 공지 · 부르기 · 범례(결정 23 특성 시험: 지금 동작을 지킨다).
// 시연 서버의 가짜 AI 가 답한다. 고정 대기 대신 화면 · 서버 상태가 바뀌기를 기다린다.
import { AGENTS, ensureTemplate, expect, myPlace, openOffice, test, walkUntilMoved } from "./support/fixtures";

const POSITION = "/api/office/me/position";

test("템플릿을 고르면 사무실에 내 캐릭터 이름표가 보인다", async ({ page }) => {
  await ensureTemplate(page);
  await expect(page.getByText(/단타 · \d+번/)).toBeVisible();
  await openOffice(page);
  await expect(page.locator("#labels .tag.me")).toHaveText("나");
});

test("채팅 칸에 초점이 있으면 방향키로 자리가 바뀌지 않는다", async ({ page }) => {
  await ensureTemplate(page);
  await openOffice(page);
  const before = await myPlace(page);
  const input = page.locator(".mention-input");
  await input.click();
  // 걸었다면 1초 안에 자리를 남긴다(player-sync). 그 요청이 3초 동안 나가지 않아야 한다.
  // (입구에서 위 · 왼쪽은 열려 있다 — 이 시험을 걷는 시험보다 먼저 둔다.)
  const sent = page.waitForRequest((r) => r.url().endsWith(POSITION), { timeout: 3_000 }).then(() => true, () => false);
  await page.keyboard.down("ArrowUp");
  await page.keyboard.down("ArrowLeft");
  const leaked = await sent;
  await page.keyboard.up("ArrowLeft");
  await page.keyboard.up("ArrowUp");
  expect(leaked, "채팅 칸에 초점이 있는데 자리를 남겼다").toBe(false);
  expect(await myPlace(page)).toEqual(before);
  // 대조: 초점을 빼면 방향키로 걷는다.
  await input.evaluate((e) => (e as HTMLElement).blur());
  expect(await walkUntilMoved(page, before), "초점을 뺀 뒤에도 걷지 않았다").toBe(true);
});

test("방향키를 누르면 내 자리(GET /api/office/me)가 바뀐다", async ({ page }) => {
  await ensureTemplate(page);
  await openOffice(page);
  const before = await myPlace(page);
  expect(await walkUntilMoved(page, before), "어느 방향키로도 자리가 바뀌지 않았다").toBe(true);
});

test("팀 방: @를 치면 담당 여섯 목록 → 고르면 칩 → 보내면 그 담당이 답한다", async ({ page }) => {
  await page.goto("/office/");
  const input = page.locator(".mention-input");
  const replies = page.locator("#chat-log .msg.agent").filter({ has: page.locator(".who strong", { hasText: /^리스크$/ }) });
  await input.click();
  await page.keyboard.type("@");
  await expect(page.locator(".mention-option")).toHaveText(AGENTS.map((a) => `@${a}`));
  const already = await replies.count();
  await page.locator(".mention-option", { hasText: "@리스크" }).click();
  const chip = input.locator(".mention-chip");
  await expect(chip).toHaveText("@리스크");
  await expect(chip).toHaveAttribute("data-agent", "risk");
  await page.keyboard.type("오늘 위험 어때?");
  await page.keyboard.press("Enter");
  await expect(page.locator("#chat-log .msg.me").last()).toHaveText("@리스크 오늘 위험 어때?");
  await expect(replies).toHaveCount(already + 1, { timeout: 30_000 });
});

test("팀 방: 멘션 없이 보내면 안내 글이 보인다", async ({ page }) => {
  await page.goto("/office/");
  await page.locator(".mention-input").click();
  await page.keyboard.type("그냥 하는 말");
  await page.keyboard.press("Enter");
  await expect(page.locator(".compose-hint")).toHaveText("@로 담당을 부르거나 공지로 보내세요");
});

test("공지를 보내면 공지 띠와 담당 여섯의 확인이 답함이 된다", async ({ page }) => {
  test.setTimeout(120_000);
  await page.goto("/office/");
  await page.locator(".notice-toggle").click();
  await expect(page.locator(".notice-mode")).toHaveText("공지로 보내기");
  await page.locator(".mention-input").click();
  await page.keyboard.type("e2e 공지: 모두 확인 바랍니다");
  await page.keyboard.press("Enter");
  const band = page.locator(".notice-band").filter({ hasText: "e2e 공지: 모두 확인 바랍니다" });
  await expect(band).toHaveCount(1);
  await expect(band.locator(".ack")).toHaveCount(6);
  await expect(band.locator('.ack[data-state="answered"]')).toHaveCount(6, { timeout: 90_000 });
  await expect(band.locator(".ack")).toHaveText(AGENTS.map((a) => `${a} ✓ 답함`));
});

test("부르기를 누르면 그 담당의 1:1 대화가 열린다", async ({ page }) => {
  await openOffice(page);
  await expect(page.locator('#chat-tabs [role="tab"][aria-selected="true"]')).toHaveText(/^팀 방/);
  // 대조(기능 H 1번): 팀 방 탭 줄에는 부르기 단추가 없다 — 전략 탭을 고른 뒤 탭 줄 끝의 "전략 부르기"를 누른다.
  await expect(page.locator("#chat-tabs button.summon")).toHaveCount(0);
  // 대조(결정 29): 탭을 고르는 것만으로 담당이 불려 오므로 도착 기다림을 탭을 누르기 전에 건다.
  const arrived = page.waitForRequest((r) => r.url().endsWith("/api/office/summon/arrived"), { timeout: 40_000 });
  await page.locator('#chat-tabs [role="tab"]', { hasText: /^전략/ }).click();
  await page.locator("#chat-tabs").getByRole("button", { name: "전략 부르기" }).click();
  // 담당이 내 곁까지 걸어온 뒤 도착을 알리고 1:1 대화를 연다(summon-scene).
  await arrived;
  await expect(page.locator('#chat-tabs [role="tab"][aria-selected="true"]')).toHaveText(/^전략/, { timeout: 40_000 });
  await expect(page.locator("#chat-input")).toBeVisible();
});

test("범례 단추가 열리고 닫힌다", async ({ page }) => {
  await page.goto("/office/");
  const toggle = page.locator("[data-legend-toggle]");
  const legend = page.locator("[data-legend]");
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(legend).toBeHidden();
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(legend).toBeVisible();
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(legend).toBeHidden();
});
