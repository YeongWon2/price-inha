#!/usr/bin/env bash
# 릴리스 묶음을 로컬에서 만든다 — 결정 27. GitHub CI 는 쓰지 않는다(결정 10).
# 기본은 지금 컴퓨터의 대상 하나. --target 으로 다른 대상(윈도 등)을 교차 빌드한다(필요한 도구가 있을 때만).
# 올리기(gh release create)는 하지 않고, 마지막에 올리는 명령을 안내만 한다(올리기는 사용자가 한다).
#   scripts/release.sh                                   지금 컴퓨터의 대상으로 짓고 dist/ 에 묶음과 .sha256 을 만든다
#   scripts/release.sh --target x86_64-pc-windows-gnu    윈도 x86_64 로 짓고 .zip(price-inha.exe)으로 묶는다
#   scripts/release.sh [--target <대상>] --dry-run       무엇을 할지만 보인다(빌드 · 쓰기 없음)
set -euo pipefail
cd "$(dirname "$0")/.."

dry=0
target=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --target) [ "$#" -ge 2 ] || { echo "--target 에는 값이 필요하다" >&2; exit 2; }; target=$2; shift ;;
    --target=*) target=${1#*=} ;;
    -h | --help) sed -n '2,7p' "$0"; exit 0 ;;
    *) echo "모르는 선택: $1 (--target <대상> · --dry-run · --help)" >&2; exit 2 ;;
  esac
  shift
done

die() { echo "release: $*" >&2; exit 1; }
run() { if [ "$dry" -eq 1 ]; then echo "[dry-run] $*"; else "$@"; fi; }

version=$(awk -F'"' '/^\[workspace\.package\]/ { p = 1; next } /^\[/ { p = 0 } p && /^version *=/ { print $2; exit }' Cargo.toml)
[ -n "$version" ] || die "Cargo.toml 의 [workspace.package] version 을 찾지 못했다"
tag="v$version"

# 대상 → 운영체제 · CPU. 대상을 주지 않으면 지금 컴퓨터.
if [ -z "$target" ]; then
  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    *) die "이 운영체제($(uname -s))는 릴리스 대상이 아니다 — 대상: macOS · Linux, 윈도는 --target x86_64-pc-windows-gnu (결정 27)" ;;
  esac
  case "$(uname -m)" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64 | amd64) arch=x86_64 ;;
    *) die "이 CPU($(uname -m))는 릴리스 대상이 아니다 — 대상: arm64 · x86_64" ;;
  esac
else
  case "$target" in
    x86_64-pc-windows-gnu | x86_64-pc-windows-msvc) os=windows; arch=x86_64 ;;
    aarch64-apple-darwin) os=macos; arch=arm64 ;;
    x86_64-apple-darwin) os=macos; arch=x86_64 ;;
    x86_64-unknown-linux-gnu) os=linux; arch=x86_64 ;;
    aarch64-unknown-linux-gnu) os=linux; arch=arm64 ;;
    *) die "모르는 대상: $target — 대상: x86_64-pc-windows-gnu · x86_64-pc-windows-msvc · aarch64-apple-darwin · x86_64-apple-darwin · x86_64-unknown-linux-gnu · aarch64-unknown-linux-gnu" ;;
  esac
fi
name="price-inha-$tag-$os-$arch"
exe=price-inha
[ "$os" = windows ] && exe=price-inha.exe
if [ -n "$target" ]; then
  bin="${CARGO_TARGET_DIR:-target}/$target/release/$exe"
  build=(cargo build --release --locked -p price-inha --target "$target")
else
  bin="${CARGO_TARGET_DIR:-target}/release/$exe"
  build=(cargo build --release --locked -p price-inha)
fi
if [ "$os" = windows ]; then archive="$name.zip"; else archive="$name.tar.gz"; fi

# 교차 빌드에 필요한 것: 대상의 표준 라이브러리(rustup target) · 윈도 gnu 는 mingw-w64 링커 · 묶는 zip.
# 깔지 않는다 — 무엇을 깔아야 하는지만 말한다.
missing=()
if [ -n "$target" ]; then
  if command -v rustup >/dev/null 2>&1; then
    rustup target list --installed 2>/dev/null | grep -qx "$target" \
      || missing+=("Rust 대상 $target — rustup target add $target")
  elif [ ! -d "$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$target" ]; then
    missing+=("Rust 대상 $target — rustup(https://rustup.rs)으로 Rust 를 깐 뒤 rustup target add $target (Homebrew 의 rust 로는 대상을 더할 수 없다)")
  fi
  case "$target" in
    x86_64-pc-windows-gnu)
      command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 \
        || missing+=("mingw-w64 링커(x86_64-w64-mingw32-gcc) — macOS: brew install mingw-w64 · Debian/Ubuntu: sudo apt install gcc-mingw-w64-x86-64") ;;
    x86_64-pc-windows-msvc)
      case "$(uname -s)" in
        MINGW* | MSYS* | CYGWIN* | Windows*) ;;
        *) missing+=("MSVC 링커 — msvc 대상은 윈도 컴퓨터(Visual Studio Build Tools)에서 짓는다. 맥 · 리눅스에서는 --target x86_64-pc-windows-gnu 를 쓴다") ;;
      esac ;;
  esac
fi
if [ "$os" = windows ] && ! command -v zip >/dev/null 2>&1; then missing+=("zip — macOS 는 기본으로 있다 · Debian/Ubuntu: sudo apt install zip"); fi

sha256_line() { # 묶음이 있는 폴더 안에서 "<해시>  <이름>"
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi
}

echo "판: $tag · 대상: $os-$arch${target:+ ($target)} · 묶음: dist/$archive"
if [ "${#missing[@]}" -gt 0 ]; then
  echo "교차 빌드에 필요한 것이 없다(이 스크립트는 깔지 않는다):"
  for m in "${missing[@]}"; do echo "  - 없음: $m"; done
  [ "$dry" -eq 1 ] || die "위의 것을 깐 뒤 다시 돌린다"
fi
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then echo "- 참고: 작업 폴더에 커밋하지 않은 변경이 있다(묶음에 들어간다)"; fi
if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then echo "- 참고: 태그 $tag 가 아직 없다 — 올리기 전에 git tag $tag 를 만든다"; fi

# 릴리스 앞 검사(2026-11-28 사용자 "릴리즈 직전에도 전체 다 돌리는 것보다 이번에 올라가는 영향이 있는 범위만"):
# 직전 릴리스 태그 뒤로 바뀐 것이 영향을 주는 범위만 돈다(같은 입력으로 통과한 검사는 캐시). 직전 태그가 없으면
# 전체. 스크립트 · 규칙 · 루트 Cargo가 바뀌었으면 영향 계산이 스스로 전체로 넓힌다. docs/dev/pipeline.md 5장.
prev=$(git tag --list 'v*' --sort=-v:refname --merged HEAD | grep -vx "$tag" | head -1 || true)
if [ -n "$prev" ]; then
  echo "- 릴리스 앞 검사: $prev 뒤로 바뀐 것의 영향 범위"
  run bash scripts/verify.sh --affected --base "$prev"
else
  echo "- 릴리스 앞 검사: 직전 릴리스 태그가 없어 전체"
  run bash scripts/verify.sh --full
fi
run "${build[@]}"
if [ "$dry" -eq 0 ]; then
  if [ -z "$target" ]; then
    got=$("$bin" --version)
    [ "$got" = "price-inha $version" ] || die "빌드한 실행 파일의 판이 다르다: $got"
  else
    [ -f "$bin" ] || die "빌드한 실행 파일이 없다: $bin"
    echo "- 참고: 다른 대상의 실행 파일은 이 컴퓨터에서 돌려 보지 못한다 — 그 컴퓨터에서 price-inha --version 을 확인한다"
  fi
fi

stage="<임시>"
if [ "$dry" -eq 0 ]; then
  stage=$(mktemp -d "${TMPDIR:-/tmp}/price-inha-release.XXXXXX")
  trap 'rm -rf "$stage"' EXIT
fi
run mkdir -p "dist" "$stage/$name"
run cp "$bin" "$stage/$name/$exe"
[ -f LICENSE ] && run cp LICENSE "$stage/$name/LICENSE"
# README 발췌: 제목과 "설치" 절만(전체 설명은 저장소에 있다).
if [ "$dry" -eq 1 ]; then echo "[dry-run] README.md 의 제목과 '## 설치' 절을 $name/README.md 로 뽑는다"
else
  awk 'NR == 1 { print; print ""; next } /^## 설치/ { p = 1; print; next } /^## / { p = 0 } p' README.md > "$stage/$name/README.md"
fi
if [ "$os" = windows ]; then
  if [ "$dry" -eq 1 ]; then echo "[dry-run] zip -qr dist/$archive $name ($name/$exe)"
  else (cd "$stage" && rm -f "$OLDPWD/dist/$archive" && zip -qrX "$OLDPWD/dist/$archive" "$name"); fi
else
  if [ "$dry" -eq 1 ]; then echo "[dry-run] tar -czf dist/$archive -C <임시> $name"
  else COPYFILE_DISABLE=1 tar -czf "dist/$archive" -C "$stage" "$name"; fi
fi
if [ "$dry" -eq 1 ]; then echo "[dry-run] dist/$archive.sha256 · dist/LATEST($tag) 를 쓴다"
else
  (cd dist && sha256_line "$archive" > "$archive.sha256")
  echo "$tag" > dist/LATEST
fi

cat <<EOF

만들었다$([ "$dry" -eq 1 ] && echo "(dry-run — 실제로는 만들지 않음)"):
  dist/$archive
  dist/$archive.sha256
  dist/LATEST

다른 대상(macOS arm64 · x86_64, Linux x86_64 · arm64)은 그 컴퓨터에서, 윈도는 --target x86_64-pc-windows-gnu 로 만들어 dist/ 를 모은다.
올리기는 이 스크립트가 하지 않는다. 사용자가 직접:
  git tag $tag && git push origin $tag
  gh release create $tag dist/price-inha-$tag-*.tar.gz dist/price-inha-$tag-*.zip dist/price-inha-$tag-*.sha256 dist/LATEST install.sh install.ps1 \\
    --title "$tag" --notes "설치(macOS · Linux): curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.sh | sh
설치(윈도 PowerShell): irm https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.ps1 | iex"
  (이미 있는 릴리스에 대상을 더할 때: gh release upload $tag dist/$archive dist/$archive.sha256 --clobber)
EOF
