#!/usr/bin/env bash
# 파이프라인 스크립트가 함께 쓰는 표. docs/dev/pipeline.md 6장과 같아야 한다.
MODULES="store secrets ai market gate"

module_dirs() { # 모듈 → 폴더들
  case "$1" in
    gate) echo "crates/gate crates/server" ;;
    store|secrets|ai|market) echo "crates/$1" ;;
    *) echo "모르는 모듈: $1" >&2; return 1 ;;
  esac
}

module_pkgs() { # 모듈 → cargo -p 인자
  case "$1" in
    gate) echo "-p pi-gate -p price-inha" ;;
    *) echo "-p pi-$1" ;;
  esac
}

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
