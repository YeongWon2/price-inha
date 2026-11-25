// 결정 메모(어제 메모) 카드 · 히스토리 결정 메모 탭(additions.md 23번 · R7-4 · C25 · V70). 시연 서버에서 되는 것만 본다:
// 정산까지 5거래일이 걸리므로 정산된 교훈 자체는 Rust · vitest 시험이 본다.
import { expect, test } from "./support/fixtures";

test("사무실 옆 패널에 어제 메모 카드가 있고 빈 상태 글이나 메모가 보인다", async ({ page }) => {
  const read = page.waitForResponse((r) => r.url().includes("/api/memos?days=7"));
  await page.goto("/office/");
  expect((await read).status()).toBe(200);
  const card = page.locator('#memos [data-role="memo-card"]');
  await expect(card.locator("h2")).toHaveText("어제 메모");
  await expect(card.locator('[data-role="empty"], [data-role="memo-list"] [data-trade]').first()).toBeVisible();
});

test("히스토리에 결정 메모 탭이 있고 고르면 목록이나 빈 상태 글이 보인다", async ({ page }) => {
  await page.goto("/history/");
  const tab = page.locator('[role="tab"][data-tab="memos"]');
  await expect(tab).toHaveText("결정 메모");
  const read = page.waitForResponse((r) => r.url().includes("/api/memos?max="));
  await tab.click();
  expect((await read).status()).toBe(200);
  await expect(tab).toHaveAttribute("aria-selected", "true");
  const panel = page.locator('[role="tabpanel"][data-tab="memos"]');
  await expect(panel).toBeVisible();
  await expect(panel.locator('[data-role="empty"], [data-role="memos"] [data-memo]').first()).toBeVisible();
});
