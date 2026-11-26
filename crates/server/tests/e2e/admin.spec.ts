// 설정 화면 보강 묶음 · 알림음 · 규칙 화면 업종표(결정 23 특성 시험).
import { expect, test } from "./support/fixtures";

async function openSettings(page: import("@playwright/test").Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: /설정 열기/ }).click();
}

test("설정 화면에 시장 · 담당 모델 · 진단 표 7줄 · 알림음이 보인다", async ({ page }) => {
  await openSettings(page);
  await expect(page.locator('[data-role="market"]')).toContainText("국내");
  await expect(page.locator('[data-role="models"] [data-agent]')).toHaveCount(6);
  await expect(page.locator('[data-role="diagnosis"] tr[data-key]')).toHaveCount(8);
  await expect(page.locator('[data-role="sound"]')).toBeVisible();
  await expect(page.locator("#sound-on")).toBeChecked();
});

test("알림음을 끄고 새로 고쳐도 꺼져 있다", async ({ page }) => {
  await openSettings(page);
  const sound = page.locator("#sound-on");
  await expect(sound).toBeChecked();
  const saved = page.waitForResponse((r) => r.url().endsWith("/api/setup/sound") && r.request().method() === "POST");
  await sound.uncheck();
  expect((await saved).ok()).toBe(true);
  await openSettings(page);
  await expect(page.locator('[data-role="sound"]')).toBeVisible();
  await expect(sound).not.toBeChecked();
});

test("규칙 화면: 업종표에 줄을 더해 저장하고 새로 고쳐도 남는다", async ({ page }) => {
  await page.goto("/rules/");
  const box = page.locator('[data-role="rules-config"]');
  await box.locator('input[name="sector-symbol"]').fill("005930");
  await box.locator('input[name="sector-name"]').fill("반도체");
  await box.locator('button[data-action="add-sector"]').click();
  await expect(box.locator('tr[data-sector="005930"]')).toContainText("반도체");
  await box.locator('button[data-action="save-config"]').click();
  await expect(box.locator('[data-role="config-result"]')).toContainText("저장했습니다");
  await page.reload();
  await expect(page.locator('[data-role="rules-config"] tr[data-sector="005930"]')).toContainText("반도체");
});
