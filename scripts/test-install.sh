#!/usr/bin/env bash
# 설치 스크립트(install.sh) 시험 — 결정 27. 네트워크를 쓰지 않고, 임시 폴더만 쓴다.
# 가짜 릴리스 묶음(작은 가짜 실행 파일 + .sha256)을 만들어 install.sh 가 깔고 · 거절하고 · 지우는지 본다.
#   bash scripts/test-install.sh
#   INSTALL_SH=dash bash scripts/test-install.sh   (install.sh 를 다른 POSIX 셸로 돌려 본다)
set -uo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
inst="$root/install.sh"
[ -f "$inst" ] || { echo "✗ install.sh 가 없다"; exit 1; }

t=$(mktemp -d "${TMPDIR:-/tmp}/pi-test-install.XXXXXX")
trap 'rm -rf "$t"' EXIT
fail=0
ok()  { printf '✓ %s\n' "$*"; }
bad() { printf '✗ %s\n' "$*"; fail=1; }

# 네트워크 차단: 받는 도구를 부르면 기록만 하고 실패하는 가짜를 PATH 앞에 둔다.
mkdir -p "$t/fakebin"
for tool in curl wget git; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/net.log"\nexit 1\n' "$tool" "$t" > "$t/fakebin/$tool"
  chmod 755 "$t/fakebin/$tool"
done
export COPYFILE_DISABLE=1

# 가짜 릴리스 묶음: make_release <폴더> <판> [실행 파일 본문]
make_release() {
  local dir="$1" ver="$2" body="${3:-}" name="price-inha-$2-linux-x86_64"
  mkdir -p "$dir/stage/$name"
  if [ -z "$body" ]; then
    body=$(printf '#!/bin/sh\ncase "$1" in\n  --version) echo "price-inha %s" ;;\n  *) echo "가짜 $*" ;;\nesac\n' "${ver#v}")
  fi
  printf '%s\n' "$body" > "$dir/stage/$name/price-inha"
  chmod 755 "$dir/stage/$name/price-inha"
  echo "가짜 라이선스" > "$dir/stage/$name/LICENSE"
  tar -czf "$dir/$name.tar.gz" -C "$dir/stage" "$name"
  (cd "$dir" && { sha256sum "$name.tar.gz" 2>/dev/null || shasum -a 256 "$name.tar.gz"; } > "$name.tar.gz.sha256")
  printf '%s' "$dir/$name.tar.gz"
}

# 설치 실행: inst_run <홈> <선택...> — 출력은 $out, 종료 코드는 $code
inst_run() {
  local home="$1"; shift
  mkdir -p "$home"
  out=$(env -i HOME="$home" PATH="$t/fakebin:/usr/bin:/bin" TMPDIR="$t" LANG="${LANG:-C.UTF-8}" \
    PRICE_INHA_OS=Linux PRICE_INHA_ARCH=x86_64 PRICE_INHA_TTY="${TTY_FILE:-$t/no-tty}" ${EXTRA_ENV:+"$EXTRA_ENV"} \
    "${INSTALL_SH:-sh}" "$inst" "$@" </dev/null 2>&1)
  code=$?
}
has() { case "$out" in *"$1"*) return 0 ;; *) return 1 ;; esac; }
show() { printf '%s\n' "$out" | sed 's/^/    /' | tail -8; }

rel=$(make_release "$t/r1" v9.9.9)

# 1 로컬 묶음으로 깔린다
H="$t/h1"; P="$t/p1"
inst_run "$H" --from "$rel" --prefix "$P"
if [ "$code" -eq 0 ] && [ -x "$P/price-inha" ] && [ "$("$P/price-inha" --version)" = "price-inha 9.9.9" ] \
  && has "SHA-256 확인" && has "price-inha setup"; then ok "로컬 묶음 --from 으로 깔린다 (sha256 확인 · setup 안내)"
else bad "로컬 묶음 --from 으로 깔린다 (종료 $code)"; show; fi
if has "PATH 에 없다" && has "export PATH=\"$P:"; then ok "PATH 에 없으면 넣는 법만 안내한다"
else bad "PATH 에 없으면 넣는 법만 안내한다"; show; fi
if [ -z "$(ls -A "$H")" ]; then ok "홈에 아무것도 쓰지 않는다 (셸 설정 파일 · 보관 폴더)"
else bad "홈에 무언가 썼다: $(ls -A "$H")"; fi

# 2 다시 깔면 앞 판을 .prev 로 남긴다
inst_run "$H" --from "$rel" --prefix "$P" --no-setup
if [ "$code" -eq 0 ] && [ -x "$P/price-inha.prev" ]; then ok "다시 깔면 앞 실행 파일을 price-inha.prev 로 남긴다"
else bad "다시 깔기 (종료 $code)"; show; fi

# 3 sha256 이 틀리면 깔지 않는다
r2=$(make_release "$t/r2" v9.9.9)
printf '%064d  %s\n' 0 "$(basename "$r2")" > "$r2.sha256"
inst_run "$t/h3" --from "$r2" --prefix "$t/p3" --no-setup
if [ "$code" -ne 0 ] && [ ! -e "$t/p3/price-inha" ] && has "SHA-256 이 맞지 않는다"; then ok "sha256 이 틀리면 깔지 않는다"
else bad "sha256 이 틀리면 깔지 않는다 (종료 $code)"; show; fi

# 4 sha256 파일이 없으면 깔지 않는다
r4=$(make_release "$t/r4" v9.9.9); rm -f "$r4.sha256"
inst_run "$t/h4" --from "$r4" --prefix "$t/p4" --no-setup
if [ "$code" -ne 0 ] && [ ! -e "$t/p4/price-inha" ] && has "체크섬 파일이 없다"; then ok "sha256 파일이 없으면 깔지 않는다"
else bad "sha256 파일이 없으면 깔지 않는다 (종료 $code)"; show; fi

# 5 다른 묶음의 sha256 이면 깔지 않는다
r5=$(make_release "$t/r5" v9.9.9)
sed 's/price-inha-v9.9.9/price-inha-v1.0.0/' "$r5.sha256" > "$r5.x" && mv "$r5.x" "$r5.sha256"
inst_run "$t/h5" --from "$r5" --prefix "$t/p5" --no-setup
if [ "$code" -ne 0 ] && [ ! -e "$t/p5/price-inha" ] && has "다른 묶음"; then ok "다른 묶음 이름의 sha256 이면 깔지 않는다"
else bad "다른 묶음 이름의 sha256 (종료 $code)"; show; fi

# 6 이 컴퓨터에서 돌지 않는 실행 파일은 놓지 않는다(확인 뒤 --version 점검)
r6=$(make_release "$t/r6" v9.9.9 "$(printf '#!/bin/sh\nexit 1\n')")
inst_run "$t/h6" --from "$r6" --prefix "$t/p6" --no-setup
if [ "$code" -ne 0 ] && [ ! -e "$t/p6/price-inha" ] && has "돌지 않는다"; then ok "돌지 않는 실행 파일은 놓지 않는다"
else bad "돌지 않는 실행 파일 (종료 $code)"; show; fi

# 7 --dry-run 은 아무것도 쓰지 않는다
inst_run "$t/h7" --from "$rel" --prefix "$t/p7" --dry-run
if [ "$code" -eq 0 ] && [ ! -e "$t/p7" ] && [ -z "$(ls -A "$t/h7")" ] && has "[dry-run]" \
  && ! ls "$t" | grep -q '^price-inha-install'; then ok "--dry-run 은 무엇을 할지만 보이고 아무것도 쓰지 않는다"
else bad "--dry-run (종료 $code)"; show; fi

# 8 --uninstall 은 실행 파일만 지운다(보관 폴더는 그대로, 위치를 알린다)
mkdir -p "$H/.price-inha"; echo 자료 > "$H/.price-inha/keep"
inst_run "$H" --uninstall --prefix "$P"
if [ "$code" -eq 0 ] && [ ! -e "$P/price-inha" ] && [ ! -e "$P/price-inha.prev" ] \
  && [ -f "$H/.price-inha/keep" ] && has "$H/.price-inha"; then ok "--uninstall 은 실행 파일만 지우고 보관 폴더 위치를 알린다"
else bad "--uninstall (종료 $code)"; show; fi

# 9 자동 실행 등록이 남아 있으면 지우지 않고 먼저 할 일을 알린다
inst_run "$t/h9" --from "$rel" --prefix "$t/p9" --no-setup
mkdir -p "$t/h9/.config/systemd/user"; touch "$t/h9/.config/systemd/user/price-inha.service"
inst_run "$t/h9" --uninstall --prefix "$t/p9"
if [ "$code" -ne 0 ] && [ -e "$t/p9/price-inha" ] && has "service uninstall"; then ok "자동 실행 등록이 있으면 지우기 전에 service uninstall 을 알린다"
else bad "등록이 남은 지우기 (종료 $code)"; show; fi

# 10 지원하지 않는 운영체제 · CPU 는 까닭을 말하고 끝난다
EXTRA_ENV="PRICE_INHA_OS=FreeBSD" inst_run "$t/h10" --from "$rel" --prefix "$t/p10" --no-setup
if [ "$code" -ne 0 ] && [ ! -e "$t/p10" ] && has "지원하지 않는 운영체제: FreeBSD"; then ok "지원하지 않는 운영체제는 까닭을 말하고 끝난다"
else bad "지원하지 않는 운영체제 (종료 $code)"; show; fi
EXTRA_ENV="PRICE_INHA_ARCH=riscv64" inst_run "$t/h10" --from "$rel" --prefix "$t/p10" --no-setup
if [ "$code" -ne 0 ] && [ ! -e "$t/p10" ] && has "지원하지 않는 CPU: riscv64"; then ok "지원하지 않는 CPU 는 까닭을 말하고 끝난다"
else bad "지원하지 않는 CPU (종료 $code)"; show; fi
EXTRA_ENV="PRICE_INHA_OS=MINGW64_NT-10.0" inst_run "$t/h10" --from "$rel" --prefix "$t/p10" --no-setup
if [ "$code" -ne 0 ] && has "윈도는 이번 범위 밖"; then ok "윈도는 범위 밖이라고 말하고 끝난다"
else bad "윈도 (종료 $code)"; show; fi

# 11 http 주소는 거절한다(받기 전에)
inst_run "$t/h11" --from "http://example.invalid/price-inha-v9.9.9-linux-x86_64.tar.gz" --prefix "$t/p11" --no-setup
if [ "$code" -ne 0 ] && has "https 주소만" && [ ! -e "$t/p11" ]; then ok "--from 의 http 주소를 거절한다"
else bad "--from http (종료 $code)"; show; fi
EXTRA_ENV="PRICE_INHA_RELEASE_URL=http://example.invalid/rel" inst_run "$t/h11" --prefix "$t/p11" --no-setup
if [ "$code" -ne 0 ] && has "https 주소만" && [ ! -e "$t/p11" ]; then ok "PRICE_INHA_RELEASE_URL 의 http 주소를 거절한다"
else bad "PRICE_INHA_RELEASE_URL http (종료 $code)"; show; fi

# 12 cargo 가 없으면 소스로 짓지 않고 Rust 설치 안내만 한다
if env -i PATH="$t/fakebin:/usr/bin:/bin" sh -c 'command -v cargo' >/dev/null 2>&1; then
  echo "- 참고: /usr/bin · /bin 에 cargo 가 있어 'cargo 없음' 시험은 건너뛴다"
else
  inst_run "$t/h12" --from-source --prefix "$t/p12" --no-setup
  if [ "$code" -ne 0 ] && has "rustup.rs" && [ ! -e "$t/p12" ]; then ok "cargo 가 없으면 Rust 설치 안내만 하고 끝난다"
  else bad "cargo 없음 (종료 $code)"; show; fi
fi

# 13 터미널이 있으면 setup 을 묻고, 예(기본)면 실행한다 · 아니오면 안내만
printf '\n' > "$t/tty-yes"
TTY_FILE="$t/tty-yes" inst_run "$t/h13" --from "$rel" --prefix "$t/p13"
if [ "$code" -eq 0 ] && grep -q '지금 실행할까요' "$t/tty-yes" && grep -q '가짜 setup' "$t/tty-yes"; then
  ok "터미널에서 묻고 기본(예)이면 price-inha setup 을 실행한다"
else bad "setup 묻기 · 예 (종료 $code)"; show; sed 's/^/    tty: /' "$t/tty-yes"; fi
printf 'n\n' > "$t/tty-no"
TTY_FILE="$t/tty-no" inst_run "$t/h13" --from "$rel" --prefix "$t/p13"
if [ "$code" -eq 0 ] && ! grep -q '가짜 setup' "$t/tty-no" && has "price-inha setup\`을 실행하세요"; then
  ok "아니오면 실행하지 않고 안내만 한다"
else bad "setup 묻기 · 아니오 (종료 $code)"; show; fi

# 14 모르는 선택은 거절한다
inst_run "$t/h14" --frm x
if [ "$code" -ne 0 ] && has "모르는 선택"; then ok "모르는 선택을 거절한다"
else bad "모르는 선택 (종료 $code)"; show; fi

# 15 install.ps1(윈도) 정적 확인 — https 만 · 해시 확인 뒤에만 놓기 · iex 창을 닫는 exit 없음 · 관리자 · 소스 짓기 · 비밀 묻기 없음
ps1="$root/install.ps1"
if [ -f "$ps1" ]; then
  code_lines=$(grep -v '^[[:space:]]*#' "$ps1")
  sha_at=$(grep -n 'Assert-Sha256 \$archive' "$ps1" | head -1 | cut -d: -f1)
  place_at=$(grep -n 'Install-Binary \$exe' "$ps1" | head -1 | cut -d: -f1)
  if ! grep -q 'http://' "$ps1" && grep -q "StartsWith('https://')" "$ps1"; then ok "install.ps1 은 https 주소만 받는다"
  else bad "install.ps1 에 http 주소가 있거나 https 검사가 없다"; fi
  if [ -n "$sha_at" ] && [ -n "$place_at" ] && [ "$sha_at" -lt "$place_at" ] && grep -q 'Get-FileHash -Algorithm SHA256' "$ps1"; then
    ok "install.ps1 은 SHA-256 을 확인한 뒤에만 놓는다"
  else bad "install.ps1 의 해시 확인 · 놓기 차례 (확인 ${sha_at:-없음}, 놓기 ${place_at:-없음})"; fi
  if ! printf '%s\n' "$code_lines" | grep -E '\bexit\b' | grep -vq 'AsFile'; then ok "install.ps1 은 파일로 돌 때만 exit 한다 (iex 창을 닫지 않는다)"
  else bad "install.ps1 에 가드 없는 exit 가 있다"; fi
  if ! printf '%s\n' "$code_lines" | grep -Eqi 'RunAs|cargo |Invoke-Expression|\biex\b|AsSecureString'; then
    ok "install.ps1 은 관리자 권한 · 소스 짓기 · 남의 스크립트 실행 · 비밀 묻기를 하지 않는다"
  else bad "install.ps1 에 금지한 것(RunAs · cargo · iex · AsSecureString)이 있다"; fi
  if grep -q "SetEnvironmentVariable('Path', \$next, 'User')" "$ps1" && grep -q 'NoPathUpdate' "$ps1" && grep -q '아직 올라온 릴리스가 없습니다' "$ps1"; then
    ok "install.ps1 은 사용자 PATH 에만 더하고(-NoPathUpdate) 릴리스가 없으면 소스로 짓지 않고 안내한다"
  else bad "install.ps1 의 사용자 PATH · 릴리스 없음 안내"; fi
else
  bad "install.ps1 이 없다"
fi

# 16 pwsh 가 있으면 install.ps1 을 가짜 묶음으로 실제로 돌려 본다(-DryRun · 해시 틀림 · 해시 파일 없음 · http)
if command -v pwsh >/dev/null 2>&1 && command -v zip >/dev/null 2>&1 && [ -f "$ps1" ]; then
  wz="$t/win"; wn="price-inha-v9.9.9-windows-x86_64"
  mkdir -p "$wz/stage/$wn" "$wz/la"
  printf 'fake\n' > "$wz/stage/$wn/price-inha.exe"
  (cd "$wz/stage" && zip -qr "$wz/$wn.zip" "$wn")
  (cd "$wz" && { sha256sum "$wn.zip" 2>/dev/null || shasum -a 256 "$wn.zip"; } > "$wn.zip.sha256")
  ps_run() { out=$(env -i HOME="$wz" PATH="$t/fakebin:$PATH" LOCALAPPDATA="$wz/la" USERPROFILE="$wz" TMPDIR="$t" \
    pwsh -NoProfile -NonInteractive -File "$ps1" "$@" </dev/null 2>&1); code=$?; }
  ps_run -From "$wz/$wn.zip" -Prefix "$wz/p1" -DryRun
  if [ "$code" -eq 0 ] && has "[dry-run]" && [ ! -e "$wz/p1" ]; then ok "pwsh: install.ps1 -DryRun 은 아무것도 쓰지 않는다"
  else bad "pwsh: install.ps1 -DryRun (종료 $code)"; show; fi
  cp "$wz/$wn.zip.sha256" "$wz/good.sha256"
  printf '%064d  %s\n' 0 "$wn.zip" > "$wz/$wn.zip.sha256"
  ps_run -From "$wz/$wn.zip" -Prefix "$wz/p2" -NoSetup -NoPathUpdate
  if [ "$code" -ne 0 ] && has "SHA-256 이 맞지 않는다" && [ ! -e "$wz/p2/price-inha.exe" ]; then ok "pwsh: install.ps1 은 sha256 이 틀리면 깔지 않는다"
  else bad "pwsh: install.ps1 sha256 틀림 (종료 $code)"; show; fi
  rm -f "$wz/$wn.zip.sha256"
  ps_run -From "$wz/$wn.zip" -Prefix "$wz/p3" -NoSetup -NoPathUpdate
  if [ "$code" -ne 0 ] && has "체크섬 파일이 없다" && [ ! -e "$wz/p3" ]; then ok "pwsh: install.ps1 은 sha256 파일이 없으면 깔지 않는다"
  else bad "pwsh: install.ps1 sha256 없음 (종료 $code)"; show; fi
  ps_run -From "http://example.invalid/$wn.zip" -Prefix "$wz/p4" -NoSetup -NoPathUpdate
  if [ "$code" -ne 0 ] && has "https 주소만" && [ ! -e "$wz/p4" ]; then ok "pwsh: install.ps1 은 http 주소를 거절한다"
  else bad "pwsh: install.ps1 http (종료 $code)"; show; fi
else
  echo "- 참고: pwsh 없음 — install.ps1 을 실제로 돌려 보는 시험은 건너뛴다(정적 확인만 했다)"
fi

# 17 release.sh --target 윈도 --dry-run: .zip(price-inha.exe)으로 묶을 계획 · 올리기 안내 · 아무것도 쓰지 않음
before=$(ls dist 2>/dev/null | tr '\n' ' ')
out=$(bash scripts/release.sh --target x86_64-pc-windows-gnu --dry-run 2>&1); code=$?
after=$(ls dist 2>/dev/null | tr '\n' ' ')
if [ "$code" -eq 0 ] && has "windows-x86_64.zip" && has "price-inha.exe" && has "--target x86_64-pc-windows-gnu" \
  && has "gh release create" && has "install.ps1" && [ "$before" = "$after" ]; then
  ok "release.sh --target x86_64-pc-windows-gnu --dry-run 은 .zip 계획과 올리기 안내만 보인다"
else bad "release.sh --target 윈도 --dry-run (종료 $code)"; show; fi
out=$(bash scripts/release.sh --target sparc-unknown-none --dry-run 2>&1); code=$?
if [ "$code" -ne 0 ] && has "모르는 대상"; then ok "release.sh 는 모르는 대상을 거절한다"
else bad "release.sh 모르는 대상 (종료 $code)"; show; fi

# 네트워크를 쓰지 않았다
if [ -s "$t/net.log" ]; then bad "받는 도구를 불렀다(네트워크): $(tr '\n' ' ' < "$t/net.log")"
else ok "네트워크 도구(curl · wget · git)를 한 번도 부르지 않았다"; fi

# shellcheck (있을 때만)
if command -v shellcheck >/dev/null 2>&1; then
  if sc=$(shellcheck -s sh install.sh 2>&1 && shellcheck scripts/release.sh scripts/test-install.sh 2>&1); then ok "shellcheck"
  else bad "shellcheck"; printf '%s\n' "$sc" | head -40; fi
else
  echo "- 참고: shellcheck 가 없어 건너뛴다"
fi

echo
[ "$fail" -eq 0 ] && echo "설치 스크립트 시험 통과" || echo "설치 스크립트 시험 실패"
exit "$fail"
