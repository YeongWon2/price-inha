// 브라우저 e2e(결정 23): verify.sh 의 "3e 브라우저 e2e" 단계가 `npm run e2e`로 돈다. 영상은 찍지 않는다.
// 시험은 잠금이 덮는 crates/server/tests/e2e/ 에 둔다(*.spec.ts). 전역 준비(support/demo-server.mjs)가
// 임시 홈으로 시연 서버(가짜 시세 · 가짜 AI · 모의계좌)를 빈 포트에 띄우고, 끝나면 끄고 임시 폴더를 지운다.
// 실패한 시험의 trace · 스크린샷은 저장소 밖 임시 폴더(outputDir)에만 남는다.
import os from "node:os";
import path from "node:path";
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "../tests/e2e",
  testMatch: "**/*.spec.ts",
  // 시험이 이 폴더 밖에 있어 `@playwright/test`를 tsconfig 의 paths 로 찾는다.
  tsconfig: "./tsconfig.json",
  globalSetup: "../tests/e2e/support/demo-server.mjs",
  outputDir: path.join(os.tmpdir(), "price-inha-e2e"),
  // 시연 서버 하나를 함께 쓰므로 차례대로 하나씩. 흔들리는 시험을 다시 돌려 숨기지 않는다.
  workers: 1,
  fullyParallel: false,
  retries: 0,
  forbidOnly: true,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: [["list"]],
  use: {
    headless: true,
    locale: "ko-KR",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
