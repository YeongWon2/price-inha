// 설정 화면 메신저 묶음(server 39차 7 · 10번, 결정 24): 시연 서버에서 묶음이 보이고, 등록을 누르면 시연 안내가 뜬다.
// 지킴이(support/fixtures.ts)가 화면 오류와 바깥 요청이 없음을 함께 본다(시연 모드는 텔레그램으로 나가지 않는다).
import { expect, test } from "./support/fixtures";

const TOKEN = "SIHEOM-E2E-TOKEN-0039";

async function openSettings(page: import("@playwright/test").Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: /설정 열기/ }).click();
}

test("설정 화면에 메신저 묶음이 보이고 꺼짐 · 가린 토큰 칸 · 슬랙 준비 중이 있다", async ({ page }) => {
  await openSettings(page);
  const box = page.locator('[data-role="messenger"]');
  await expect(box).toBeVisible();
  await expect(box.locator('[data-role="messenger-state"]')).toHaveAttribute("data-state", "off");
  await expect(box.locator("input#messenger-token")).toHaveAttribute("type", "password");
  await expect(box.locator("input#messenger-token")).toHaveAttribute("autocomplete", "new-password");
  await expect(box.locator('[data-role="messenger-slack"]')).toContainText("준비 중");
});

test("시연 모드에서 토큰을 등록하려 하면 시연 안내가 뜨고 꺼짐 그대로다", async ({ page }) => {
  await openSettings(page);
  const box = page.locator('[data-role="messenger"]');
  await box.locator("input#messenger-token").fill(TOKEN);
  const sent = page.waitForResponse(
    (r) => r.url().endsWith("/api/messenger/telegram") && r.request().method() === "POST",
  );
  await box.locator('button[data-action="messenger-register"]').click();
  expect((await sent).status()).toBe(409);
  await expect(box.locator('[role="alert"]')).toContainText("시연 모드에서는 메신저를 쓰지 않습니다");
  await expect(box.locator('[data-role="messenger-state"]')).toHaveAttribute("data-state", "off");
  await expect(box.locator("input#messenger-token")).toHaveValue("");
});
