// 사무실 24차: 익절 · 손절 연출 표시의 뜻이 범례에 보인다(시연 모드에서 늘 같은 것만 본다 — 순찰 · 연출이 뜨는 때는
// 난수 · 매매에 달려 있어 여기서 기다리지 않는다. 그 동작은 vitest · Rust 시험이 가짜 시계로 본다).
import { expect, test } from "./support/fixtures";

test("범례를 열면 익절 · 손절 연출 표시의 뜻이 보인다", async ({ page }) => {
  await page.goto("/office/");
  await page.locator("[data-legend-toggle]").click();
  const legend = page.locator("[data-legend]");
  await expect(legend).toBeVisible();
  await expect(legend.locator('[data-mark="profit"] [data-part="label"]')).toContainText("익절");
  await expect(legend.locator('[data-mark="loss"] [data-part="label"]')).toContainText("손절");
});
