// 설정 화면 "알림" 묶음의 "팀 논의" 줄(server 41차 2번, 결정 26): 시연 서버에서 그 줄이 폰 칸 없이 보이고, 메신저 칸을
// 끄고 저장하면 새로 고쳐도 꺼져 있다. 지킴이(support/fixtures.ts)가 화면 오류와 바깥 요청이 없음을 함께 본다.
// 시연 서버 하나를 함께 쓴다 — 팀 논의는 다른 시험이 보지 않는다(시연 모드는 메신저를 돌리지 않는다).
import { expect, test } from "./support/fixtures";

async function openSettings(page: import("@playwright/test").Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: /설정 열기/ }).click();
}

test("설정 화면 알림 묶음에 팀 논의 줄이 폰 칸 없이 보이고 끄고 저장하면 새로 고쳐도 꺼져 있다", async ({ page }) => {
  await openSettings(page);
  const box = page.locator('[data-role="notify"]');
  const talk = box.locator('tr[data-role="notify-talk"]');
  await expect(talk).toBeVisible();
  await expect(talk).toContainText("팀 논의");
  await expect(talk.locator('input[name="push"]')).toHaveCount(0);
  const messenger = talk.locator('input[name="messenger"]');
  await expect(messenger).toBeChecked();
  await messenger.uncheck();
  const saved = page.waitForResponse(
    (r) => r.url().endsWith("/api/notify/settings") && r.request().method() === "POST",
  );
  await box.locator('button[data-action="notify-save"]').click();
  expect((await saved).ok()).toBe(true);
  await expect(box.locator('[data-role="notify-message"]')).toContainText("저장했습니다");

  await openSettings(page);
  const again = page.locator('[data-role="notify"] tr[data-role="notify-talk"]');
  await expect(again).toContainText("팀 논의");
  await expect(again.locator('input[name="messenger"]')).not.toBeChecked();
});
