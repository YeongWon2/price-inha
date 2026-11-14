#!/usr/bin/env bash
# G2 기계 검사. 0으로 끝나야 "끝났다"고 말할 수 있다. docs/dev/pipeline.md 5장.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
fail=0
ok()  { printf '✓ %s\n' "$*"; }
bad() { printf '✗ %s\n' "$*"; fail=1; }
run() { # run <이름> <명령...>
  local name="$1"; shift
  if out=$("$@" 2>&1); then ok "$name"; else bad "$name"; printf '%s\n' "$out" | tail -40; fi
}

run "1 모양 (cargo fmt)" cargo fmt --all --check
run "2a clippy (실행 코드, 엄격)" cargo clippy --workspace --lib --bins -- -D warnings
run "2b clippy (테스트)" cargo clippy --workspace --tests -- -D warnings \
  -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic -A missing_docs
run "3 테스트" cargo test --workspace

# 4 잠긴 테스트
for m in $MODULES; do
  if git rev-parse -q --verify "refs/tags/red/$m" >/dev/null; then
    # shellcheck disable=SC2046
    if ! git diff --quiet "red/$m" -- $(module_test_dirs "$m"); then
      bad "4 잠긴 테스트가 바뀜: $m"; git diff --stat "red/$m" -- $(module_test_dirs "$m")
    elif [ -n "$(git ls-files -o --exclude-standard -- $(module_test_dirs "$m"))" ]; then
      bad "4 잠긴 뒤 새 테스트 파일: $m"; git ls-files -o --exclude-standard -- $(module_test_dirs "$m")
    else ok "4 잠긴 테스트 그대로: $m"; fi
  else
    printf -- '- 4 %s: red 태그 없음 (아직 G1 전)\n' "$m"
  fi
done

# 5 필수 번호
for m in $MODULES; do
  [ -f "docs/dev/modules/$m.md" ] || { bad "5 지시서 없음: $m"; continue; }
  miss=$(missing_ids "$m")
  if [ -n "$miss" ]; then bad "5 필수 번호 빠짐 ($m): $(echo $miss)"; else ok "5 필수 번호: $m"; fi
done

# 6 의존 방향
PY=""
for c in python3.14 python3.13 python3.12 python3.11 python3; do
  if command -v "$c" >/dev/null && "$c" -c 'import tomllib' 2>/dev/null; then PY="$c"; break; fi
done
if [ -z "$PY" ]; then bad "6 아키텍처 · 컨벤션: Python 3.11 이상이 없다"
elif out=$("$PY" scripts/check_arch.py 2>&1); then ok "6 아키텍처 · 컨벤션"
else bad "6 아키텍처 · 컨벤션"; printf '%s\n' "$out"; fi

# 7 비밀 글자
sec=$(git ls-files -co --exclude-standard | grep -vE '^scripts/verify\.sh$' | tr '\n' '\0' \
  | xargs -0 grep -nIE 'sk-ant-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{32,}|gh[pousr]_[A-Za-z0-9]{30,}|AGE-SECRET-KEY-1[A-Z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY|eyJ[A-Za-z0-9_-]{20,}\.eyJ[A-Za-z0-9_-]{20,}\.' 2>/dev/null)
files=$(git ls-files -co --exclude-standard | grep -E '(^|/)(\.env(\..*)?|auth\.json)$' | grep -vE '\.env\.example$')
if [ -n "$sec" ] || [ -n "$files" ]; then bad "7 비밀 글자"; printf '%s\n%s\n' "$sec" "$files" | cut -c1-120; else ok "7 비밀 글자 없음"; fi

# 8 바깥 주소
oh_any=0
for m in $MODULES; do
  oh=$(outside_hosts "$m")
  if [ -n "$oh" ]; then bad "8 테스트에 바깥 주소: $m"; printf '%s\n' "$oh"; oh_any=1; fi
done
[ "$oh_any" -eq 0 ] && ok "8 테스트에 바깥 주소 없음"

# 9 검사 끄기
off=$(grep -rnE '#!?\[(cfg_attr\([^]]*)?(ignore|allow)\b' crates --include='*.rs' 2>/dev/null)
if [ -n "$off" ]; then bad "9 검사를 끈 곳"; printf '%s\n' "$off"; else ok "9 검사를 끈 곳 없음"; fi

echo
[ "$fail" -eq 0 ] && echo "G2 통과" || echo "G2 실패"
exit "$fail"
