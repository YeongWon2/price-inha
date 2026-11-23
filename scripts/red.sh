#!/usr/bin/env bash
# G1 red 확인: scripts/red.sh <모듈> [--lock]
# 통과 조건: 컴파일된다 · 테스트가 있다 · 테스트가 실패한다 · 필수 번호가 다 있다 · 바깥 주소가 없다.
# --lock (동작 잠금 모드, QMIT 결정 2): "테스트가 실패한다" 대신 "지금 구현에서 모두 통과한다"를 본다
#   (리팩토링 · 옮기기는 바꾸기 전과 뒤 모두 통과로 증명한다).
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
m="${1:?모듈 이름을 준다: $MODULES}"
lockmode=0
[ "${2:-}" = "--lock" ] && lockmode=1
fail=0
say() { printf '%s\n' "$*"; }

# shellcheck disable=SC2046
if ! cargo test $(module_pkgs "$m") --no-run >/tmp/pi-red-$m.log 2>&1; then
  say "✗ 컴파일 안 됨 (뼈대가 있어야 red다)"; tail -20 /tmp/pi-red-$m.log; exit 1
fi
say "✓ 컴파일됨"

# shellcheck disable=SC2046
n=$(grep -rhcE '^\s*#\[(tokio::)?test' $(module_test_dirs "$m") 2>/dev/null | awk '{s+=$1} END{print s+0}')
# shellcheck disable=SC2046
nw=$(grep -rhcE '^\s*(it|test)(\.each\([^)]*\))?\(' --include='*.ts' $(module_test_dirs "$m") 2>/dev/null | awk '{s+=$1} END{print s+0}')
n=$((n + nw))
if [ "$n" -lt 1 ]; then say "✗ 테스트 없음"; fail=1; else say "✓ 테스트 ${n}개 (화면 ${nw}개)"; fi

# shellcheck disable=SC2046
out=$(cargo test $(module_pkgs "$m") --no-fail-fast 2>&1); code=$?
passed=$(printf '%s\n' "$out" | grep -E '^test result:' | sed -E 's/.* ([0-9]+) passed.*/\1/' | awk '{s+=$1} END{print s+0}')
failed=$(printf '%s\n' "$out" | grep -E '^test result:' | sed -E 's/.* ([0-9]+) failed.*/\1/' | awk '{s+=$1} END{print s+0}')
# 화면 시험(vitest)도 red 에 든다: Rust 시험이나 화면 시험 가운데 하나라도 실패해야 red.
wcode=0
for w in $(module_web "$m"); do
  if ! command -v npm >/dev/null; then say "✗ 화면 시험: npm 없음 — 고치기: Node.js LTS 를 설치한다"; fail=1; continue; fi
  if ! web_deps "$w" >/tmp/pi-red-web-$m.log 2>&1; then say "✗ 화면 의존성 (npm ci) 실패"; tail -20 /tmp/pi-red-web-$m.log; fail=1; continue; fi
  if wout=$(npm --prefix "$w" test --silent 2>&1); then say "  화면 시험 (vitest): 통과"
  else wcode=1; say "  화면 시험 (vitest): 실패 — $(printf '%s\n' "$wout" | grep -E 'Tests? .*(failed|passed)' | tail -1)"; fi
done
if [ "$lockmode" -eq 1 ]; then
  if [ "$code" -eq 0 ] && [ "$wcode" -eq 0 ]; then say "✓ 동작 잠금: 지금 구현에서 모두 통과 (통과 ${passed}개, Rust)"
  else
    say "✗ 동작 잠금인데 지금 구현에서 실패하는 시험이 있다 — 고치기: 시험이 지금 동작을 그대로 적게 한다 (src 는 고치지 않는다)"
    printf '%s\n' "$out" | grep -E '^test .* \.\.\. FAILED$' | head -20 | sed 's/^/    /'
    fail=1
  fi
elif [ "$code" -eq 0 ] && [ "$wcode" -eq 0 ]; then say "✗ 테스트가 이미 통과함 (red가 아니다)"; fail=1
else say "✓ red: 실패 ${failed}개 · 통과 ${passed}개 (Rust)"; fi
if [ "$lockmode" -eq 0 ] && [ "$passed" -gt 0 ]; then
  say "  참고: 구현 전인데 통과하는 테스트 — 아무것도 확인하지 않는 테스트인지 본다"
  printf '%s\n' "$out" | grep -E '^test .* \.\.\. ok$' | head -20 | sed 's/^/    /'
fi

# 테스트 코드도 G2 의 모양 · clippy 검사를 미리 통과해야 한다 (잠근 뒤에는 고칠 수 없다).
# shellcheck disable=SC2046
if o=$(cargo fmt $(module_pkgs "$m") --check 2>&1); then say "✓ 모양 (cargo fmt)"
else say "✗ 모양이 다름 — 고치기: cargo fmt $(module_pkgs "$m")"; fail=1; fi
# shellcheck disable=SC2046
if o=$(cargo clippy $(module_pkgs "$m") --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used \
  -A clippy::panic -A missing_docs -A clippy::todo -A unused -A clippy::diverging_sub_expression 2>&1); then say "✓ 테스트 clippy"
else say "✗ 테스트 clippy — 고치기: 아래 경고를 고친다 (예: 테스트 이름에 라틴 대문자를 쓰지 않는다)"; printf '%s\n' "$o" | grep -E '^(error|warning)' | sort -u | head -20; fail=1; fi

miss=$(missing_ids "$m")
if [ -n "$miss" ]; then say "✗ 필수 번호 빠짐: $(echo $miss)"; fail=1; else say "✓ 필수 번호 모두 있음"; fi

oh=$(outside_hosts "$m")
if [ -n "$oh" ]; then say "✗ 테스트에 바깥 주소"; printf '%s\n' "$oh"; fail=1; else say "✓ 바깥 주소 없음"; fi

sfx=""; [ "$lockmode" -eq 1 ] && sfx=" (동작 잠금)"
[ "$fail" -eq 0 ] && say "G1 통과: $m$sfx" || say "G1 실패: $m$sfx"
exit "$fail"
