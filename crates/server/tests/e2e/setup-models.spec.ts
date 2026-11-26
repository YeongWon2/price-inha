// 설정 화면의 담당 모델 고르기(server 57차 · R18-4 · C32 · V78): 담당 여섯 줄 · 직접 입력으로 저장 · 새로 고쳐도 남음 ·
// 기본값으로 되돌리기(시연 서버 — 이 컴퓨터에서 연다).
import { expect, test } from "./support/fixtures";

async function openSettings(page: import("@playwright/test").Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: /설정 열기/ }).click();
}

test("담당 모델: 직접 입력으로 저장하면 새로 고쳐도 남고, 기본값으로 되돌릴 수 있다", async ({ page }) => {
  await openSettings(page);
  const box = page.locator('[data-role="models"]');
  await expect(box.locator("tr[data-agent]")).toHaveCount(6);
  const scout = box.locator('tr[data-agent="scout"]');
  await scout.locator("select").selectOption("__custom__");
  await scout.locator("input").fill("zz-e2e-model-57");
  const saved = page.waitForResponse((r) => r.url().endsWith("/api/setup/models") && r.request().method() === "POST");
  await box.locator('button[data-action="save-models"]').click();
  expect((await saved).ok()).toBe(true);
  await expect(box.locator('[data-role="models-result"]')).toContainText("저장했습니다");
  await openSettings(page);
  const again = page.locator('[data-role="models"] tr[data-agent="scout"]');
  await expect(again.locator("select")).toHaveValue("__custom__");
  await expect(again.locator("input")).toHaveValue("zz-e2e-model-57");
  const reset = page.waitForResponse((r) => r.url().endsWith("/api/setup/models") && r.request().method() === "POST");
  await page.locator('[data-role="models"] button[data-action="reset-models"]').click();
  expect((await reset).ok()).toBe(true);
  await expect(page.locator('[data-role="models"] tr[data-agent="scout"] select')).not.toHaveValue("__custom__");
});
