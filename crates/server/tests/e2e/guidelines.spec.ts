// 규칙 화면의 "팀 지침(AI가 배운 것)" 묶음(기능 L · 결정 30, 특성 시험). 시연 서버에서 확실한 것만 본다:
// 묶음이 보이고, 1층 규칙과 다르다는 한 줄이 있다(시연을 막 띄운 서버에는 배운 지침이 없을 수 있다).
import { expect, test } from "./support/fixtures";

test("규칙 화면에 팀 지침 묶음과 1층과 다르다는 한 줄이 보인다", async ({ page }) => {
  await page.goto("/rules/");
  const box = page.locator('[data-role="guidelines"]');
  await expect(box).toBeVisible();
  await expect(box).toContainText("팀 지침(AI가 배운 것)");
  await expect(box.locator('[data-role="guidelines-note"]')).toContainText("승인 없이");
});
