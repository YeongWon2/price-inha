#!/usr/bin/env bash
# 파이프라인 스크립트가 함께 쓰는 표. docs/dev/pipeline.md 6장과 같아야 한다.
# 모든 모듈(그래프 docs/dev/graph.toml 과 같은 순서). 폴더는 crates/<모듈>, 패키지는 pi-<모듈>(server 는 price-inha).
ALL_MODULES="store secrets ai market paper gate server template rules approval exec data trigger agent journal knowledge backup office notify"

module_dirs() { # 모듈 → 폴더들
  case " $ALL_MODULES " in
    *" $1 "*) echo "crates/$1" ;;
    *) echo "모르는 모듈: $1" >&2; return 1 ;;
  esac
}

module_pkgs() { # 모듈 → cargo -p 인자
  case "$1" in
    server) echo "-p price-inha" ;;
    *) echo "-p pi-$1" ;;
  esac
}

# 검사하는 모듈: 크레이트가 이미 있는 것만(새 모듈은 그 크레이트를 처음 만드는 커밋부터 들어온다).
MODULES=""
for _m in $ALL_MODULES; do
  [ -f "crates/$_m/Cargo.toml" ] && MODULES="$MODULES $_m"
done
MODULES="${MODULES# }"
unset _m

module_test_dirs() {
  local d out=""
  for d in $(module_dirs "$1"); do out="$out $d/tests"; done
  echo "$out"
}

# 지시서의 "## 필수 번호" 아래 줄에서 번호를 뽑는다.
required_ids() {
  awk '/^## 필수 번호/{p=1;next} /^## /{p=0} p' "docs/dev/modules/$1.md" \
    | grep -oE '\b[A-Z]{1,2}[0-9]+(-[0-9]+)?\b' | sort -u
}

# 테스트의 "// 검증:" 주석에 적힌 번호.
covered_ids() {
  # shellcheck disable=SC2046
  grep -rhE '^\s*// 검증:' $(module_test_dirs "$1") 2>/dev/null \
    | grep -oE '\b[A-Z]{1,2}[0-9]+(-[0-9]+)?\b' | sort -u
}

missing_ids() { comm -23 <(required_ids "$1") <(covered_ids "$1"); }

outside_hosts() { # 테스트가 실제 바깥 주소를 쓰는가
  # shellcheck disable=SC2046
  grep -rnE 'tossinvest\.com|anthropic\.com|openai\.com|googleapis\.com' $(module_test_dirs "$1") 2>/dev/null
}

# 키처럼 생긴 글자. 이 줄 자체가 걸리지 않게 모양을 쪼개 쓴다.
SECRET_RX='sk-ant''-[A-Za-z0-9_-]{20,}|sk''-[A-Za-z0-9]{32,}|gh[pousr]''_[A-Za-z0-9]{30,}|AGE-SECRET''-KEY-1[A-Z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE'' KEY|eyJ[A-Za-z0-9_-]{20,}\.eyJ[A-Za-z0-9_-]{20,}\.'

find_python() { # tomllib가 있는 Python
  local c
  for c in python3.14 python3.13 python3.12 python3.11 python3; do
    if command -v "$c" >/dev/null && "$c" -c 'import tomllib' 2>/dev/null; then echo "$c"; return; fi
  done
}

# 검사 규칙 전체의 지문. 규칙이 바뀌면 자체 시험을 다시 돌려야 한다.
rules_hash() {
  cat scripts/*.sh scripts/*.py docs/dev/architecture.toml docs/dev/graph.toml Cargo.toml clippy.toml 2>/dev/null | shasum -a 256 | cut -d' ' -f1
}

# 지시서에 "## 원본 동작 근거" 절이 있고, 조사 문서를 가리키거나 "원본에 없음"이라고 적었는가.
origin_ok() {
  awk '/^## 원본 동작 근거/{p=1;next} /^## /{p=0} p' "docs/dev/modules/$1.md" \
    | grep -qE 'docs/research/[a-z-]+\.md|원본에 없음'
}
