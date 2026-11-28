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

# 동작 잠금(QMIT 결정 2)에서 기준 판(PI_RED_BASE: 다시 열기 전 잠금 판)이 있으면, 바뀐 · 새 시험 대상만 돌린다.
# src 를 고치지 않는 차수라 바뀌지 않은 시험은 같은 코드에서 이미 통과했고, G2 가 모듈 전체를 다시 돈다.
# src · Cargo.toml · web 이 바뀌었거나, 대상을 모르는 시험 파일(도움 모듈 등)이 바뀌면 전부 돈다.
rtargets=""; rfull=1; vtests=""; vall=1
if [ "$lockmode" -eq 1 ] && [ -n "${PI_RED_BASE:-}" ] && git rev-parse -q --verify "${PI_RED_BASE}^{commit}" >/dev/null; then
  rfull=0; vall=0
  td="crates/$m/tests"
  # 기대는 크레이트 · 루트 빌드 파일이 기준 뒤에 바뀌었으면(다른 모듈의 커밋 등) 바뀌지 않은 시험도 결과가 달라질 수 있다 → 전부.
  PYD=$(find_python)
  deps=$("$PYD" scripts/affected.py --deps "$m" 2>/dev/null) || deps="ALL"
  if [ "$deps" = ALL ]; then rfull=1; vall=1
  else
    dpaths="Cargo.toml Cargo.lock clippy.toml rust-toolchain rust-toolchain.toml .cargo"
    for d in $deps; do dpaths="$dpaths crates/$d"; done
    # shellcheck disable=SC2086
    if [ -n "$( { git diff --name-only "$PI_RED_BASE" -- $dpaths; git ls-files -o --exclude-standard -- $dpaths; } 2>/dev/null)" ]; then
      say "  동작 잠금 범위: 기대는 크레이트 · 빌드 파일이 기준 뒤에 바뀜 — 모듈 시험 전부"; rfull=1; vall=1
    fi
  fi
  for f in $( { git diff --name-only "$PI_RED_BASE" -- "crates/$m"; git ls-files -o --exclude-standard -- "crates/$m"; } | sort -u); do
    case "$f" in
      "$td"/web/*.test.ts) [ -f "$f" ] && vtests="$vtests tests/web/${f##*/}" ;;
      "$td"/web/*) vall=1 ;;
      "$td"/e2e/*) ;; # G1 은 e2e 를 돌리지 않는다(G2 가 돈다)
      "$td"/*/*) d=${f#"$td"/}; d=${d%%/*}
        if [ -f "$td/$d/main.rs" ]; then rtargets="$rtargets $d"; else rfull=1; fi ;;
      "$td"/*.rs) n=$(basename "$f" .rs)
        if grep -rlE "(mod $n;|$n\.rs)" "$td" --include='*.rs' 2>/dev/null | grep -qv "/$n.rs$"; then rfull=1
        elif [ -f "$f" ]; then rtargets="$rtargets $n"; fi ;;
      "$td"/*) rfull=1 ;;
      *) rfull=1; vall=1 ;;
    esac
  done
fi
code=0; passed=0; failed=0
if [ "$lockmode" -eq 1 ]; then
  PY=$(find_python)
  if [ "$rfull" -eq 0 ] && [ -z "$rtargets" ]; then
    say "건너뜀: Rust 시험 — 까닭(동작 잠금: 기준 판 대비 바뀐 Rust 시험 대상 없음 · src 그대로. G2 가 모듈 전체를 돈다)"
  else
    sel="$m"; [ "$rfull" -eq 0 ] && sel="$m:$(echo $rtargets | tr ' ' '\n' | sort -u | paste -sd, -)"
    [ "$rfull" -eq 0 ] && say "  동작 잠금 범위: 바뀐 · 새 시험 대상만 — ${sel#*:}"
    out=$("$PY" scripts/cargo_tests.py "$sel" 2>&1); code=$?
    printf '%s\n' "$out" | grep -E '^(✓|✗|  흔들림)' | sed 's/^/  /'
    passed=$(printf '%s\n' "$out" | sed -nE 's/.*통과 ([0-9]+)개.*/\1/p' | awk '{s+=$1} END{print s+0}')
  fi
else
  # shellcheck disable=SC2046
  out=$(cargo test $(module_pkgs "$m") --no-fail-fast 2>&1); code=$?
  passed=$(printf '%s\n' "$out" | grep -E '^test result:' | sed -E 's/.* ([0-9]+) passed.*/\1/' | awk '{s+=$1} END{print s+0}')
  failed=$(printf '%s\n' "$out" | grep -E '^test result:' | sed -E 's/.* ([0-9]+) failed.*/\1/' | awk '{s+=$1} END{print s+0}')
fi
# 화면 시험(vitest)도 red 에 든다: Rust 시험이나 화면 시험 가운데 하나라도 실패해야 red.
wcode=0
for w in $(module_web "$m"); do
  if ! command -v npm >/dev/null; then say "✗ 화면 시험: npm 없음 — 고치기: Node.js LTS 를 설치한다"; fail=1; continue; fi
  if ! web_deps "$w" >/tmp/pi-red-web-$m.log 2>&1; then say "✗ 화면 의존성 (npm ci) 실패"; tail -20 /tmp/pi-red-web-$m.log; fail=1; continue; fi
  # 브라우저 e2e(결정 23)는 G1 에서 돌리지 않는다(느리다 — G2 가 돈다). 잠그기 전에 시험 파일이 문법 · 형(tsc)과
  # 시험 린트(기다리지 않은 Promise 등)는 맞는지만 본다 — 잠근 뒤에는 시험을 고칠 수 없다.
  if tout=$(npm --prefix "$w" run --silent typecheck 2>&1); then say "✓ 화면 형 검사 (tsc: 화면 · vitest 시험 · e2e 시험)"
  else say "✗ 화면 형 검사 (tsc) — 고치기: 아래 오류를 고친다(뼈대에 쓰는 이름이 있어야 한다)"; printf '%s\n' "$tout" | head -20; fail=1; fi
  if tout=$(npm --prefix "$w" run --silent lint:tests 2>&1); then say "✓ 시험 린트 (eslint: tests/web · tests/e2e)"
  else say "✗ 시험 린트 — 고치기: 아래 [규칙]을 시험 코드에서 고친다(주석으로 끄지 않는다)"; printf '%s\n' "$tout" | tail -20; fail=1; fi
  if [ "$vall" -eq 0 ] && [ -z "$vtests" ]; then
    say "  건너뜀: 화면 시험 (vitest) — 까닭(동작 잠금: 바뀐 화면 시험 없음 · 화면 소스 그대로. G2 가 돈다)"
  elif [ "$vall" -eq 1 ]; then
    if wout=$(npm --prefix "$w" test --silent 2>&1); then say "  화면 시험 (vitest): 통과"
    else wcode=1; say "  화면 시험 (vitest): 실패 — $(printf '%s\n' "$wout" | grep -E 'Tests? .*(failed|passed)' | tail -1)"; fi
  else
    # shellcheck disable=SC2086
    if wout=$(npm --prefix "$w" exec -- vitest run $vtests 2>&1); then say "  화면 시험 (vitest: 바뀐 시험만): 통과"
    else wcode=1; say "  화면 시험 (vitest: 바뀐 시험만): 실패 — $(printf '%s\n' "$wout" | grep -E 'Tests? .*(failed|passed)' | tail -1)"; fi
  fi
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
# 뼈대(todo!) 때문에 unused 는 끄되, 시험 도움 파일의 안 쓰는 항목(dead_code)은 G2 2b 처럼 잡는다(server 15차에서 놓침).
# shellcheck disable=SC2046
if o=$(cargo fmt $(module_pkgs "$m") --check 2>&1); then say "✓ 모양 (cargo fmt)"
else say "✗ 모양이 다름 — 고치기: cargo fmt $(module_pkgs "$m")"; fail=1; fi
# shellcheck disable=SC2046
if o=$(cargo clippy $(module_pkgs "$m") --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used \
  -A clippy::panic -A missing_docs -A clippy::todo -A unused -D dead_code -A clippy::diverging_sub_expression 2>&1); then say "✓ 테스트 clippy"
else say "✗ 테스트 clippy — 고치기: 아래 경고를 고친다 (예: 테스트 이름에 라틴 대문자를 쓰지 않는다)"; printf '%s\n' "$o" | grep -E '^(error|warning)' | sort -u | head -20; fail=1; fi

miss=$(missing_ids "$m")
if [ -n "$miss" ]; then say "✗ 필수 번호 빠짐: $(echo $miss)"; fail=1; else say "✓ 필수 번호 모두 있음"; fi

oh=$(outside_hosts "$m")
if [ -n "$oh" ]; then say "✗ 테스트에 바깥 주소"; printf '%s\n' "$oh"; fail=1; else say "✓ 바깥 주소 없음"; fi

sfx=""; [ "$lockmode" -eq 1 ] && sfx=" (동작 잠금)"
[ "$fail" -eq 0 ] && say "G1 통과: $m$sfx" || say "G1 실패: $m$sfx"
exit "$fail"
