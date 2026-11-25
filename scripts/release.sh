#!/usr/bin/env bash
# 릴리스 묶음을 로컬에서 만든다 — 결정 27. GitHub CI 는 쓰지 않는다(결정 10).
# 지금 컴퓨터의 대상 하나만 만든다(교차 빌드 없음). 다른 대상은 그 컴퓨터에서 이 스크립트를 돌린다.
# 올리기(gh release create)는 하지 않고, 마지막에 올리는 명령을 안내만 한다(올리기는 사용자가 한다).
#   scripts/release.sh            빌드하고 dist/ 에 묶음과 .sha256 을 만든다
#   scripts/release.sh --dry-run  무엇을 할지만 보인다(빌드 · 쓰기 없음)
set -euo pipefail
cd "$(dirname "$0")/.."

dry=0
case "${1:-}" in
  "") ;;
  --dry-run) dry=1 ;;
  -h | --help) sed -n '2,6p' "$0"; exit 0 ;;
  *) echo "모르는 선택: $1 (--dry-run · --help)" >&2; exit 2 ;;
esac

die() { echo "release: $*" >&2; exit 1; }
run() { if [ "$dry" -eq 1 ]; then echo "[dry-run] $*"; else "$@"; fi; }

version=$(awk -F'"' '/^\[workspace\.package\]/ { p = 1; next } /^\[/ { p = 0 } p && /^version *=/ { print $2; exit }' Cargo.toml)
[ -n "$version" ] || die "Cargo.toml 의 [workspace.package] version 을 찾지 못했다"
tag="v$version"

case "$(uname -s)" in
  Darwin) os=macos ;;
  Linux) os=linux ;;
  *) die "이 운영체제($(uname -s))는 릴리스 대상이 아니다 — 대상: macOS · Linux (결정 27)" ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) arch=arm64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *) die "이 CPU($(uname -m))는 릴리스 대상이 아니다 — 대상: arm64 · x86_64" ;;
esac
name="price-inha-$tag-$os-$arch"
bin="${CARGO_TARGET_DIR:-target}/release/price-inha"

sha256_line() { # 묶음이 있는 폴더 안에서 "<해시>  <이름>"
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi
}

echo "판: $tag · 대상: $os-$arch · 묶음: dist/$name.tar.gz"
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then echo "- 참고: 작업 폴더에 커밋하지 않은 변경이 있다(묶음에 들어간다)"; fi
if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then echo "- 참고: 태그 $tag 가 아직 없다 — 올리기 전에 git tag $tag 를 만든다"; fi

run cargo build --release --locked -p price-inha
if [ "$dry" -eq 0 ]; then
  got=$("$bin" --version)
  [ "$got" = "price-inha $version" ] || die "빌드한 실행 파일의 판이 다르다: $got"
fi

stage="<임시>"
if [ "$dry" -eq 0 ]; then
  stage=$(mktemp -d "${TMPDIR:-/tmp}/price-inha-release.XXXXXX")
  trap 'rm -rf "$stage"' EXIT
fi
run mkdir -p "dist" "$stage/$name"
run cp "$bin" "$stage/$name/price-inha"
[ -f LICENSE ] && run cp LICENSE "$stage/$name/LICENSE"
# README 발췌: 제목과 "설치" 절만(전체 설명은 저장소에 있다).
if [ "$dry" -eq 1 ]; then echo "[dry-run] README.md 의 제목과 '## 설치' 절을 $name/README.md 로 뽑는다"
else
  awk 'NR == 1 { print; print ""; next } /^## 설치/ { p = 1; print; next } /^## / { p = 0 } p' README.md > "$stage/$name/README.md"
fi
if [ "$dry" -eq 1 ]; then echo "[dry-run] tar -czf dist/$name.tar.gz -C <임시> $name"
else COPYFILE_DISABLE=1 tar -czf "dist/$name.tar.gz" -C "$stage" "$name"; fi
if [ "$dry" -eq 1 ]; then echo "[dry-run] dist/$name.tar.gz.sha256 · dist/LATEST($tag) 를 쓴다"
else
  (cd dist && sha256_line "$name.tar.gz" > "$name.tar.gz.sha256")
  echo "$tag" > dist/LATEST
fi

cat <<EOF

만들었다$([ "$dry" -eq 1 ] && echo "(dry-run — 실제로는 만들지 않음)"):
  dist/$name.tar.gz
  dist/$name.tar.gz.sha256
  dist/LATEST

다른 대상(macOS arm64 · x86_64, Linux x86_64 · arm64)은 그 컴퓨터에서 이 스크립트를 돌려 dist/ 를 모은다.
올리기는 이 스크립트가 하지 않는다. 사용자가 직접:
  git tag $tag && git push origin $tag
  gh release create $tag dist/price-inha-$tag-*.tar.gz dist/price-inha-$tag-*.tar.gz.sha256 dist/LATEST install.sh \\
    --title "$tag" --notes "설치: curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/YeongWon2/price-inha/main/install.sh | sh"
  (이미 있는 릴리스에 대상을 더할 때: gh release upload $tag dist/$name.tar.gz dist/$name.tar.gz.sha256 --clobber)
EOF
