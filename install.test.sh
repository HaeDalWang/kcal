#!/bin/sh
# install.sh 확인(macOS): 가짜 kcal.app 묶음을 file:// 주소로 내려받게 해 네트워크 없이 돈다.
# 확인하는 것: 정상 설치·다시 설치(덮어쓰기)·체크섬 틀리면 아무것도 안 깔림·묶음에 앱이 없으면 실패·
# 켜져 있는 앱은 끄고 덮어씀·안 꺼지면 아무것도 안 바꾸고 실패.
# osascript는 가짜(PATH 앞)라 이 Mac에서 실제로 켜진 kcal을 끄지 않는다.
# 실행: pnpm installer:test
set -eu
cd "$(dirname "$0")"
[ "$(uname -s)" = Darwin ] || { echo "install-test: macOS only, skipped"; exit 0; }

work=$(mktemp -d "${TMPDIR:-/tmp}/kcal-install-test.XXXXXX")
APP_BIN="$work/apps/kcal.app/Contents/MacOS/kcal"
trap 'pkill -f "^$APP_BIN" 2>/dev/null; rm -rf "$work"' EXIT

# 가짜 osascript: 호출을 적고, QUIT_OK=1이면 테스트용 앱 프로세스만 끝낸다.
mkdir -p "$work/bin"
cat >"$work/bin/osascript" <<EOS
#!/bin/sh
echo called >>"$work/osascript.log"
[ "\${QUIT_OK:-}" = 1 ] && pkill -f "^$APP_BIN"
exit 0
EOS
chmod +x "$work/bin/osascript"
PATH="$work/bin:$PATH"
export PATH
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

# 켜져 있는 앱: argv[0]을 설치된 경로로 바꾼 sleep을 띄운다(복사한 시스템 실행 파일은 macOS가 막는다).
start_app() {
  (bash -c 'exec -a "$1" sleep 600' _ "$APP_BIN" &)
  sleep 0.3
  pgrep -f "^$APP_BIN" >/dev/null || fail "could not start the fake running app"
}

start_app
pack v4
QUIT_OK=1 run || fail "install over a running app exited non-zero"
grep -q called "$work/osascript.log" || fail "running app was not asked to quit"
pgrep -f "^$APP_BIN" >/dev/null && fail "running app was not stopped"
[ "$(installed)" = v4 ] || fail "install over a running app did not put v4 in place"

start_app
pack v5
QUIT_OK=0 run && fail "install went on while the app kept running"
grep -q "still running" "$work/out" || fail "still-running message not shown"
pgrep -f "^$APP_BIN" >/dev/null || fail "app that refused to quit was killed"
[ "$(installed)" = v4 ] || fail "app that refused to quit was replaced"

echo "install-test: ok (install, reinstall, bad checksum, missing app, running app quit, refuses to quit)"
