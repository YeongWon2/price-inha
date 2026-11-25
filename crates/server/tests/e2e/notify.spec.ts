// 설정 화면 "알림" 묶음(server 40차 4 · 6번, 결정 24): 시연 서버에서 한 종류의 폰 알림을 끄고 저장하면 새로 고쳐도
// 꺼져 있다. 시연 서버 하나를 함께 쓰므로 끈 종류는 다른 시험이 보지 않는 하루 정리다.
import { expect, test } from "./support/fixtures";

async function openSettings(page: import("@playwright/test").Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: /설정 열기/ }).click();
}

test("설정 화면에 알림 묶음이 보이고 종류 여섯 줄 · 잠긴 줄 셋 · 조용한 시간 칸이 있다", async ({ page }) => {
  await openSettings(page);
  const box = page.locator('[data-role="notify"]');
  await expect(box).toBeVisible();
  await expect(box.locator("tr[data-kind]")).toHaveCount(6);
  await expect(box.locator('tr[data-kind][data-locked="true"]')).toHaveCount(3);
  await expect(box.locator("#notify-quiet-on")).not.toBeChecked();
  await expect(box.locator('button[data-action="notify-save"]')).toBeVisible();
});

test("한 종류의 폰 알림을 끄고 저장하면 새로 고쳐도 꺼져 있다", async ({ page }) => {
  await openSettings(page);
  const box = page.locator('[data-role="notify"]');
  const push = box.locator('tr[data-kind="daily_summary"] input[name="push"]');
  await expect(push).toBeChecked();
  await push.uncheck();
  const saved = page.waitForResponse(
    (r) => r.url().endsWith("/api/notify/settings") && r.request().method() === "POST",
  );
  await box.locator('button[data-action="notify-save"]').click();
  expect((await saved).ok()).toBe(true);
  await expect(box.locator('[data-role="notify-message"]')).toContainText("저장했습니다");

  await openSettings(page);
  const again = page.locator('[data-role="notify"] tr[data-kind="daily_summary"]');
  await expect(again.locator('input[name="push"]')).not.toBeChecked();
  await expect(again.locator('input[name="messenger"]')).toBeChecked();
});
