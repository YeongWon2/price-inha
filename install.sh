#!/bin/sh
# 가격인하(price-inha) 한 줄 설치 — macOS · Linux.
#
#   curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.sh | sh
#
# 하는 일: 운영체제 · CPU를 알아내고, 릴리스 묶음을 받아 SHA-256으로 확인한 뒤 실행 파일 하나를
# ~/.local/bin/price-inha 에 놓는다. 릴리스가 없거나 --from-source 면 cargo 로 소스에서 짓는다.
# 하지 않는 일: sudo · 셸 설정 파일 고치기 · 남의 설치 스크립트 실행 · 비밀값 묻기(그것은 `price-inha setup`).
# 근거: docs/dev/decisions.md 결정 24 · 27, docs/research/install-and-cli.md 8.2.
set -eu
umask 077

REPO_URL_DEFAULT="https://github.com/YeongWon2/price-inha"

say() { printf '%s\n' "$*"; }
die() { printf '가격인하 설치: %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
가격인하(price-inha) 설치

쓰는 법: install.sh [선택]

  --version <태그>     그 릴리스를 깐다(예: v0.1.0). 없으면 최신 릴리스
  --from <파일|주소>   이 릴리스 묶음(.tar.gz)을 깐다. 옆에 <같은 이름>.sha256 이 있어야 한다(https 주소만)
  --from-source        릴리스 대신 소스로 짓는다(cargo 필요)
  --source <폴더>      소스로 지을 때 이 저장소 폴더를 쓴다(없으면 얕게 받는다)
  --prefix <폴더>      놓을 곳(기본 ~/.local/bin)
  --no-setup           끝에 `price-inha setup` 을 묻지 않는다
  --uninstall          실행 파일만 지운다(보관 폴더는 그대로 둔다)
  --dry-run            무엇을 할지만 보이고 아무것도 쓰지 않는다
  -h, --help           이 도움말

환경 변수:
  PRICE_INHA_RELEASE_URL  릴리스 파일이 있는 https 폴더 주소(기본: GitHub 릴리스)
  PRICE_INHA_REPO_URL     소스를 받을 https 저장소 주소
  PRICE_INHA_PREFIX       --prefix 와 같다
  PRICE_INHA_HOME         보관 폴더(지울 때 알려 주는 위치. 기본 ~/.price-inha)

지원: macOS(arm64 · x86_64), Linux(x86_64 · arm64). 윈도는 PowerShell 에서 install.ps1 을 쓴다.
EOF
}

need() { [ "$1" -ge 2 ] || die "$2 에는 값이 필요하다"; }

prefix="${PRICE_INHA_PREFIX:-${HOME:?HOME 이 없다}/.local/bin}"
from=""
from_source=0
source_dir=""
version=""
dry=0
uninstall=0
no_setup=0
release_url="${PRICE_INHA_RELEASE_URL:-}"
repo_url="${PRICE_INHA_REPO_URL:-$REPO_URL_DEFAULT}"
# 시험용: 묻는 터미널을 바꾼다(scripts/test-install.sh).
tty_dev="${PRICE_INHA_TTY:-/dev/tty}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    -h | --help) usage; exit 0 ;;
    --dry-run) dry=1 ;;
    --uninstall) uninstall=1 ;;
    --from-source) from_source=1 ;;
    --no-setup) no_setup=1 ;;
    --from) need "$#" "$1"; from=$2; shift ;;
    --from=*) from=${1#*=} ;;
    --source) need "$#" "$1"; source_dir=$2; shift ;;
    --source=*) source_dir=${1#*=} ;;
    --prefix) need "$#" "$1"; prefix=$2; shift ;;
    --prefix=*) prefix=${1#*=} ;;
    --version) need "$#" "$1"; version=$2; shift ;;
    --version=*) version=${1#*=} ;;
    *) die "모르는 선택: $1 (install.sh --help 로 쓰는 법을 본다)" ;;
  esac
  shift
done

[ -n "$prefix" ] || die "--prefix 가 비었다"
dest="$prefix/price-inha"

run() { # run <명령...>: --dry-run 이면 보이기만 한다
  if [ "$dry" -eq 1 ]; then say "[dry-run] $*"; else "$@"; fi
}

require_https() {
  case "$1" in
    https://*) ;;
    *) die "https 주소만 받는다: $1" ;;
  esac
}

norm_version() {
  [ -n "$1" ] || die "판 이름이 비었다"
  case "$1" in v*) v=$1 ;; *) v="v$1" ;; esac
  case "$v" in *[!0-9A-Za-z._-]*) die "판 이름이 이상하다: $1" ;; esac
  printf '%s' "$v"
}

# --- 지우기: 실행 파일만. 보관 폴더는 건드리지 않는다.
do_uninstall() {
  plist="$HOME/Library/LaunchAgents/kr.priceinha.server.plist"
  unit="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/price-inha.service"
  for def in "$plist" "$unit"; do
    if [ -e "$def" ]; then
      die "자동 실행 등록이 남아 있다($def). 먼저 \`price-inha service uninstall\` 을 돌린 뒤 다시 지운다"
    fi
  done
  found=0
  for f in "$dest" "$dest.prev"; do
    if [ -e "$f" ]; then found=1; run rm -f "$f"; [ "$dry" -eq 1 ] || say "지웠다: $f"; fi
  done
  [ "$found" -eq 1 ] || say "깔린 실행 파일이 없다: $dest"
  data="${PRICE_INHA_HOME:-$HOME/.price-inha}"
  say "보관 폴더는 그대로 두었다: $data"
  say "  (키 · 기록 · 모의계좌가 들어 있다. 자료까지 지우려면 직접 지운다 — 되돌릴 수 없다)"
  exit 0
}

if [ "$uninstall" -eq 1 ]; then do_uninstall; fi

if [ "$(id -u)" = "0" ] && [ "$dry" -eq 0 ]; then
  die "root 로 돌리지 않는다 — 사용자 권한으로 사용자 폴더에 깐다(sudo 없이 다시 돌린다)"
fi

# --- 대상 판별
sys="${PRICE_INHA_OS:-$(uname -s)}"
mach="${PRICE_INHA_ARCH:-$(uname -m)}"
case "$sys" in
  Darwin) os=macos ;;
  Linux) os=linux ;;
  MINGW* | MSYS* | CYGWIN* | Windows*)
    die "윈도($sys)에서는 이 스크립트 대신 PowerShell 한 줄 설치를 쓴다: irm https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.ps1 | iex" ;;
  *) die "지원하지 않는 운영체제: $sys — 지원: macOS(arm64 · x86_64), Linux(x86_64 · arm64). README 의 '소스로 짓기'를 따른다" ;;
esac
case "$mach" in
  arm64 | aarch64) arch=arm64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *) die "지원하지 않는 CPU: $mach — 지원: arm64 · x86_64. README 의 '소스로 짓기'를 따른다" ;;
esac
target="$os-$arch"

[ -z "$version" ] || version=$(norm_version "$version")
case "$from" in
  "" | https://*) ;;
  *://*) die "https 주소만 받는다: $from" ;;
esac
[ -n "$from" ] || [ "$from_source" -eq 1 ] || {
  if [ -n "$release_url" ]; then require_https "$release_url"; fi
  require_https "$repo_url"
}

# --- 받기 · 확인 도구
fetch() { # fetch <https 주소> <저장할 파일>
  require_https "$1"
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget --https-only -q -O "$2" "$1"
  else
    die "curl 이나 wget 이 필요하다"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  else return 1; fi
}

verify_sha256() { # verify_sha256 <묶음> <.sha256 파일> — 맞지 않으면 아무것도 깔지 않고 멈춘다
  want=$(awk 'NR == 1 { print tolower($1) }' "$2")
  case "$want" in "" | *[!0-9a-f]*) die "체크섬 파일이 이상하다: $2" ;; esac
  [ "${#want}" -eq 64 ] || die "체크섬 파일이 이상하다: $2"
  named=$(awk 'NR == 1 { sub(/^\*/, "", $2); print $2 }' "$2")
  if [ -n "$named" ] && [ "$named" != "$(basename "$1")" ]; then
    die "체크섬 파일이 다른 묶음의 것이다: $named"
  fi
  got=$(sha256_of "$1") || die "SHA-256 도구(sha256sum · shasum)가 없다 — 확인 없이는 깔지 않는다"
  got=$(printf '%s' "$got" | tr 'A-F' 'a-f')
  [ "$want" = "$got" ] || die "SHA-256 이 맞지 않는다 — 설치하지 않는다 (기대 $want, 받은 것 $got)"
  say "SHA-256 확인: $(basename "$1")"
}

# --- 놓기 · 점검
check_binary() { # check_binary <실행 파일> [기대 판]
  out=$("$1" --version 2>/dev/null) || die "받은 실행 파일이 이 컴퓨터($target)에서 돌지 않는다"
  case "$out" in "price-inha "*) ;; *) die "받은 실행 파일이 price-inha 가 아니다" ;; esac
  if [ -n "${2:-}" ] && [ "$out" != "price-inha ${2#v}" ]; then
    die "판이 다르다: 기대 ${2#v}, 받은 것 ${out#price-inha }"
  fi
}

place() { # place <실행 파일>: 같은 폴더의 임시 이름으로 복사한 뒤 이름을 바꾼다
  mkdir -p "$prefix"
  if [ -f "$dest" ]; then cp -p "$dest" "$dest.prev"; fi
  new="$prefix/.price-inha.new.$$"
  cp "$1" "$new"
  chmod 755 "$new"
  mv -f "$new" "$dest"
  say "놓았다: $dest ($("$dest" --version))"
}

install_archive() { # install_archive <묶음> <.sha256> [기대 판]
  verify_sha256 "$1" "$2"
  if tar -tzf "$1" | grep -Eq '^/|(^|/)\.\.(/|$)'; then die "묶음 안에 이상한 경로가 있다 — 설치하지 않는다"; fi
  mkdir "$tmp/x"
  tar -xzf "$1" -C "$tmp/x"
  bin=$(find "$tmp/x" -maxdepth 2 -type f -name price-inha | head -n 1)
  [ -n "$bin" ] || die "묶음 안에 price-inha 실행 파일이 없다"
  chmod 755 "$bin"
  check_binary "$bin" "${3:-}"
  place "$bin"
}

rust_guide() {
  say "Rust(cargo)가 없어 소스로 지을 수 없다."
  say "  1) https://rustup.rs 의 안내대로 Rust 를 설치한다(이 스크립트는 남의 설치 스크립트를 대신 받아 돌리지 않는다)."
  say "  2) 새 터미널에서 이 설치를 다시 돌린다: install.sh --from-source"
}

build_from_source() {
  command -v cargo >/dev/null 2>&1 || { rust_guide; exit 1; }
  src=$source_dir
  if [ -z "$src" ]; then
    case "$0" in */*) here=$(cd "$(dirname "$0")" && pwd) ;; *) here="" ;; esac
    if [ -n "$here" ] && [ -f "$here/Cargo.lock" ] && [ -f "$here/crates/server/Cargo.toml" ]; then src=$here; fi
  fi
  if [ -z "$src" ]; then
    command -v git >/dev/null 2>&1 || die "git 이 필요하다(소스를 받는다)"
    require_https "$repo_url"
    say "소스를 얕게 받는다: $repo_url ${version:-(기본 가지)}"
    if [ -n "$version" ]; then
      git clone -q --depth 1 --branch "$version" "$repo_url.git" "$tmp/src"
    else
      git clone -q --depth 1 "$repo_url.git" "$tmp/src"
    fi
    src="$tmp/src"
  fi
  [ -f "$src/crates/server/Cargo.toml" ] || die "가격인하 저장소가 아니다: $src"
  say "소스로 짓는다(몇 분 걸린다): $src"
  cargo install --locked --quiet --path "$src/crates/server" --root "$tmp/root"
  check_binary "$tmp/root/bin/price-inha"
  place "$tmp/root/bin/price-inha"
}

# --- 무엇을 할지(dry-run 이면 여기서 끝)
if [ "$dry" -eq 1 ]; then
  say "[dry-run] 대상: $target"
  if [ -n "$from" ]; then
    say "[dry-run] 묶음: $from 와 $from.sha256"
  elif [ "$from_source" -eq 1 ]; then
    say "[dry-run] 소스로 짓기: cargo install --locked --path <소스>/crates/server (cargo 없으면 Rust 설치 안내만)"
  else
    say "[dry-run] 릴리스: ${release_url:-$repo_url/releases} 의 price-inha-${version:-<최신>}-$target.tar.gz 와 .sha256 (https 만). 없으면 소스로 짓기"
  fi
  say "[dry-run] SHA-256 확인 — 맞지 않으면 설치하지 않는다"
  say "[dry-run] 놓기: $dest (sudo 없음 · 셸 설정 파일은 고치지 않음)"
  [ "$no_setup" -eq 1 ] || say "[dry-run] 끝에 \`price-inha setup\` 을 실행할지 묻는다(터미널이 있을 때)"
  exit 0
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/price-inha-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' INT TERM HUP

if [ -n "$from" ]; then
  name=$(basename "$from")
  case "$from" in
    https://*)
      fetch "$from" "$tmp/$name" || die "받지 못했다: $from"
      fetch "$from.sha256" "$tmp/$name.sha256" || die "체크섬 파일을 받지 못했다: $from.sha256 — 확인 없이는 깔지 않는다"
      ;;
    *)
      [ -f "$from" ] || die "파일이 없다: $from"
      [ -f "$from.sha256" ] || die "체크섬 파일이 없다: $from.sha256 — 확인 없이는 깔지 않는다"
      cp "$from" "$tmp/$name"
      cp "$from.sha256" "$tmp/$name.sha256"
      ;;
  esac
  install_archive "$tmp/$name" "$tmp/$name.sha256"
elif [ "$from_source" -eq 1 ]; then
  build_from_source
else
  if [ -n "$release_url" ]; then base=${release_url%/}
  elif [ -n "$version" ]; then base="$repo_url/releases/download/$version"
  else base="$repo_url/releases/latest/download"; fi
  require_https "$base"
  got_release=1
  if [ -z "$version" ]; then
    if fetch "$base/LATEST" "$tmp/LATEST" 2>/dev/null; then
      version=$(norm_version "$(head -n 1 "$tmp/LATEST" | tr -d ' \r\n')")
    else got_release=0; fi
  fi
  if [ "$got_release" -eq 1 ]; then
    name="price-inha-$version-$target.tar.gz"
    fetch "$base/$name" "$tmp/$name" 2>/dev/null || got_release=0
  fi
  if [ "$got_release" -eq 1 ]; then
    fetch "$base/$name.sha256" "$tmp/$name.sha256" || die "체크섬 파일을 받지 못했다: $base/$name.sha256 — 확인 없이는 깔지 않는다"
    install_archive "$tmp/$name" "$tmp/$name.sha256" "$version"
  else
    say "이 컴퓨터($target)에 맞는 릴리스 묶음을 찾지 못했다 — 소스로 짓는다"
    build_from_source
  fi
fi

# --- PATH 안내(셸 설정 파일은 고치지 않는다)
case ":$PATH:" in
  *":$prefix:"*) ;;
  *)
    say ""
    say "$prefix 가 PATH 에 없다. 셸 설정 파일(~/.zshrc 또는 ~/.bashrc)에 아래 한 줄을 직접 더한다:"
    say "  export PATH=\"$prefix:\$PATH\""
    ;;
esac

# --- 다음 단계: 터미널이 있으면 묻는다(파이프로 받았어도 /dev/tty 로)
say ""
if [ "$no_setup" -eq 0 ] && [ -e "$tty_dev" ] && (true <"$tty_dev" >>"$tty_dev") 2>/dev/null; then
  printf '`price-inha setup`을 지금 실행할까요? [Y/n] ' >>"$tty_dev"
  answer=""
  read -r answer <"$tty_dev" || answer="n"
  case "$answer" in
    "" | y | Y | yes | YES | 예 | 네 | ㅇ)
      "$dest" setup <"$tty_dev" >>"$tty_dev" 2>&1 || say "\`price-inha setup\`이 끝나지 않았다 — 나중에 다시 실행한다"
      exit 0
      ;;
  esac
fi
say "다음: \`price-inha setup\`을 실행하세요 (키 · 종목 · 메신저를 묻습니다)."
say "      화면은 \`price-inha open\`, 키 없이 먼저 보려면 \`price-inha run --demo\`."
