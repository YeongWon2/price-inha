#!/usr/bin/env bash
# G1 red 확인: scripts/red.sh <모듈>
# 통과 조건: 컴파일된다 · 테스트가 있다 · 테스트가 실패한다 · 필수 번호가 다 있다 · 바깥 주소가 없다.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
m="${1:?모듈 이름을 준다: $MODULES}"
fail=0
say() { printf '%s\n' "$*"; }

# shellcheck disable=SC2046
if ! cargo test $(module_pkgs "$m") --no-run >/tmp/pi-red-$m.log 2>&1; then
  say "✗ 컴파일 안 됨 (뼈대가 있어야 red다)"; tail -20 /tmp/pi-red-$m.log; exit 1
fi
say "✓ 컴파일됨"

# shellcheck disable=SC2046
n=$(grep -rhcE '^\s*#\[(tokio::)?test' $(module_test_dirs "$m") 2>/dev/null | awk '{s+=$1} END{print s+0}')
if [ "$n" -lt 1 ]; then say "✗ 테스트 없음"; fail=1; else say "✓ 테스트 $n개"; fi

# shellcheck disable=SC2046
out=$(cargo test $(module_pkgs "$m") --no-fail-fast 2>&1); code=$?
passed=$(printf '%s\n' "$out" | grep -E '^test result:' | sed -E 's/.* ([0-9]+) passed.*/\1/' | awk '{s+=$1} END{print s+0}')
failed=$(printf '%s\n' "$out" | grep -E '^test result:' | sed -E 's/.* ([0-9]+) failed.*/\1/' | awk '{s+=$1} END{print s+0}')
if [ "$code" -eq 0 ]; then say "✗ 테스트가 이미 통과함 (red가 아니다)"; fail=1
else say "✓ red: 실패 $failed개 · 통과 $passed개"; fi
if [ "$passed" -gt 0 ]; then
  say "  참고: 구현 전인데 통과하는 테스트 — 아무것도 확인하지 않는 테스트인지 본다"
  printf '%s\n' "$out" | grep -E '^test .* \.\.\. ok$' | head -20 | sed 's/^/    /'
fi

miss=$(missing_ids "$m")
if [ -n "$miss" ]; then say "✗ 필수 번호 빠짐: $(echo $miss)"; fail=1; else say "✓ 필수 번호 모두 있음"; fi

oh=$(outside_hosts "$m")
if [ -n "$oh" ]; then say "✗ 테스트에 바깥 주소"; printf '%s\n' "$oh"; fail=1; else say "✓ 바깥 주소 없음"; fi

[ "$fail" -eq 0 ] && say "G1 통과: $m" || say "G1 실패: $m"
exit "$fail"
