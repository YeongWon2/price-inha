// 브라우저 e2e(결정 23): verify.sh 의 "3e 브라우저 e2e" 단계가 `npm run e2e`로 돈다. 영상은 찍지 않는다.
// 시험은 잠금이 덮는 crates/server/tests/e2e/ 에 둔다(*.spec.ts). 전역 준비(support/demo-server.mjs)가
// 임시 홈으로 시연 서버(가짜 시세 · 가짜 AI · 모의계좌)를 빈 포트에 띄우고, 끝나면 끄고 임시 폴더를 지운다.
// 실패한 시험의 trace · 스크린샷은 저장소 밖 임시 폴더(outputDir)에만 남는다.
// (기능 H 3번) 그 임시 폴더는 실행마다 따로 만든다(`price-inha-e2e-XXXXXX`): 여러 작업 폴더가 동시에 돌려도 서로
// 지우지 않는다. 일꾼 프로세스는 환경 변수 PI_E2E_RUN_DIR 를 물려받아 같은 폴더를 쓴다. 시연 서버 로그도 그 안에
// 남긴다. 모두 통과하면 지우고, 실패하면 남겨 경로를 알린다(support/run-dir.mjs).
import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { defineConfig, devices } from "@playwright/test";

process.env.PI_E2E_RUN_DIR ??= mkdtempSync(path.join(os.tmpdir(), "price-inha-e2e-"));
const RUN_DIR = process.env.PI_E2E_RUN_DIR;

export default defineConfig({
  testDir: "../tests/e2e",
  testMatch: "**/*.spec.ts",
  // 시험이 이 폴더 밖에 있어 `@playwright/test`를 tsconfig 의 paths 로 찾는다.
  tsconfig: "./tsconfig.json",
  globalSetup: "../tests/e2e/support/demo-server.mjs",
  outputDir: path.join(RUN_DIR, "results"),
  // 시연 서버 하나를 함께 쓰므로 차례대로 하나씩. 흔들리는 시험을 다시 돌려 숨기지 않는다.
  workers: 1,
  fullyParallel: false,
  retries: 0,
  forbidOnly: true,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: [["list"], ["../tests/e2e/support/run-dir.mjs"]],
  use: {
    headless: true,
    locale: "ko-KR",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
