#!/bin/sh
# kcal installer (macOS). Usage:
#   curl -fsSL https://raw.githubusercontent.com/HaeDalWang/kcal/main/install.sh | sh
# Environment variables (optional):
#   KCAL_DOWNLOAD_BASE  download URL (default: latest GitHub release)
#   KCAL_INSTALL_DIR    install folder (default: /Applications, or ~/Applications if not writable)
#   KCAL_NO_OPEN=1      do not open the app after install
# Rationale: docs/decisions/0020-unsigned-macos-release.md
set -eu

ARCHIVE=kcal-macos-universal.tar.gz
APP=kcal.app
QUIT_WAIT_SECS=10

fail() {
  echo "kcal install failed: $*" >&2
  exit 1
}

pick_install_dir() {
  if [ -n "${KCAL_INSTALL_DIR:-}" ]; then
    mkdir -p "$KCAL_INSTALL_DIR" || fail "could not create the folder: $KCAL_INSTALL_DIR"
    echo "$KCAL_INSTALL_DIR"
  elif [ -w /Applications ]; then
    echo /Applications
  else
    mkdir -p "$HOME/Applications" || fail "could not create the folder: $HOME/Applications"
    echo "Installing to ~/Applications (/Applications is not writable)." >&2
    if [ -d "/Applications/$APP" ]; then
      echo "Note: the older /Applications/$APP is not updated; remove it to avoid running the old copy." >&2
    fi
    echo "$HOME/Applications"
  fi
}

# Match only the installed kcal.app executable, not dev builds or other processes named kcal.
# Checks /Applications, ~/Applications and the target folder. pgrep -f takes a regex, so escape the path.
app_running() {
  for d in /Applications "$HOME/Applications" "$1"; do
    esc=$(printf '%s' "$d/$APP/Contents/MacOS/" | sed 's/[][\.*^$+?(){}|/]/\\&/g')
    pgrep -f "^$esc" >/dev/null 2>&1 && return 0
  done
  return 1
}

# SIGTERM instead of `osascript quit`: Apple Events to another app can raise a
# Terminal → kcal Automation prompt. kcal writes its files by atomic rename.
quit_running_app() {
  app_running "$1" || return 0
  for d in /Applications "$HOME/Applications" "$1"; do
    esc=$(printf '%s' "$d/$APP/Contents/MacOS/" | sed 's/[][\.*^$+?(){}|/]/\\&/g')
    pkill -TERM -f "^$esc" >/dev/null 2>&1 || true
  done
  i=0
  while app_running "$1" && [ "$i" -lt "$QUIT_WAIT_SECS" ]; do
    sleep 1
    i=$((i + 1))
  done
  if app_running "$1"; then
    fail "kcal is still running. Quit it from its menu bar icon and run this again."
  fi
}

main() {
  [ "$(uname -s)" = Darwin ] || fail "kcal runs on macOS only; other systems are not supported yet."

  base=${KCAL_DOWNLOAD_BASE:-https://github.com/HaeDalWang/kcal/releases/latest/download}
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/kcal-install.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT
  trap 'exit 1' HUP INT TERM

  echo "Downloading: $base/$ARCHIVE"
  curl -fsSL -o "$tmp/$ARCHIVE" "$base/$ARCHIVE" || fail "could not download $base/$ARCHIVE (no network, or no release is published yet: see https://github.com/HaeDalWang/kcal/releases)"
  curl -fsSL -o "$tmp/$ARCHIVE.sha256" "$base/$ARCHIVE.sha256" ||
    fail "could not download the checksum file $base/$ARCHIVE.sha256"

  (cd "$tmp" && shasum -a 256 -c "$ARCHIVE.sha256") || fail "the SHA-256 checksum does not match, so nothing was installed."

  tar -xzf "$tmp/$ARCHIVE" -C "$tmp" || fail "could not unpack the download."
  [ -d "$tmp/$APP" ] || fail "the download does not contain $APP."

  dir=$(pick_install_dir)
  target="$dir/$APP"
  staged="$dir/.$APP.new.$$"

  quit_running_app "$dir"
  rm -rf "$staged"
  mv "$tmp/$APP" "$staged" || fail "could not write to $dir."
  rm -rf "$target" || fail "could not remove the old $target; remove it and run again."
  mv "$staged" "$target" || fail "could not move the new app to $target; remove it and run again."
  xattr -dr com.apple.quarantine "$target" 2>/dev/null || true

  echo "Installed: $target"
  echo "kcal is a menu bar app: click its icon in the top-right menu bar (no window, no Dock icon)."
  echo "On the first limit check macOS may ask once to read your Claude Code login. The window names security, not kcal: click Allow. See the README."
  if [ "${KCAL_NO_OPEN:-}" != 1 ]; then
    open "$target" || echo "Could not open kcal automatically. Open it from $dir."
  fi
}

main "$@"
