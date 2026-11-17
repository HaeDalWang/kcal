#!/bin/sh
# install.sh 확인(macOS): 가짜 kcal.app 묶음을 file:// 주소로 내려받게 해 네트워크 없이 돈다.
# 확인하는 것: 정상 설치·다시 설치(덮어쓰기)·체크섬 틀리면 아무것도 안 깔림·묶음에 앱이 없으면 실패.
# 실행: pnpm installer:test
set -eu
cd "$(dirname "$0")"
[ "$(uname -s)" = Darwin ] || { echo "install-test: macOS only, skipped"; exit 0; }

work=$(mktemp -d "${TMPDIR:-/tmp}/kcal-install-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
ARCHIVE=kcal-macos-universal.tar.gz

pack() { # $1 = 표시 문자열(앱 안 파일 내용). 묶음과 체크섬을 $work/dl에 만든다.
  rm -rf "$work/src" "$work/dl" && mkdir -p "$work/src/kcal.app/Contents/MacOS" "$work/dl"
  printf '%s' "$1" >"$work/src/kcal.app/Contents/MacOS/kcal"
  tar -czf "$work/dl/$ARCHIVE" -C "$work/src" kcal.app
  (cd "$work/dl" && shasum -a 256 "$ARCHIVE" >"$ARCHIVE.sha256")
}

run() {
  KCAL_DOWNLOAD_BASE="file://$work/dl" KCAL_INSTALL_DIR="$work/apps" KCAL_NO_OPEN=1 \
    sh ./install.sh >"$work/out" 2>&1
}

fail() { echo "install-test FAILED: $*" >&2; cat "$work/out" >&2; exit 1; }
installed() { cat "$work/apps/kcal.app/Contents/MacOS/kcal"; }

pack v1
run || fail "first install exited non-zero"
[ "$(installed)" = v1 ] || fail "first install did not put v1 in place"

pack v2
run || fail "reinstall exited non-zero"
[ "$(installed)" = v2 ] || fail "reinstall did not replace v1 with v2"
ls -a "$work/apps" | grep -q '^\.kcal\.app\.new' && fail "staging folder left behind"

pack v3
echo "0000000000000000000000000000000000000000000000000000000000000000  $ARCHIVE" >"$work/dl/$ARCHIVE.sha256"
run && fail "bad checksum was accepted"
[ "$(installed)" = v2 ] || fail "bad checksum still changed the installed app"

rm -rf "$work/src" && mkdir -p "$work/src/other"
tar -czf "$work/dl/$ARCHIVE" -C "$work/src" other
(cd "$work/dl" && shasum -a 256 "$ARCHIVE" >"$ARCHIVE.sha256")
run && fail "archive without kcal.app was accepted"
grep -q "does not contain kcal.app" "$work/out" || fail "missing-app message not shown"
[ "$(installed)" = v2 ] || fail "archive without the app changed the installed app"

echo "install-test: ok (install, reinstall, bad checksum, missing app)"
