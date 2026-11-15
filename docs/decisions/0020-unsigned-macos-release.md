# 0020. 서명 없는 macOS 릴리스와 한 줄 설치

- 상태: 결정(구현함). 릴리스 게시는 사람이 한다.
- 날짜: 2026-11-11
- 앞선 결정: [0001](0001-initial-stack.md) 스택, CI 규칙(`.github/workflows/ci.yml`의 SHA 고정)

## 문제

레포에 릴리스도 릴리스 workflow도 없어서 사용자가 kcal을 설치할 방법이 없다. Apple Developer 계정이 없어 Developer ID 서명과 공증을 할 수 없다. 브라우저로 받은 서명 없는 앱은 Gatekeeper가 막는다.

## 사용자 결정 (2026-11-11)

- 서명·공증 없이 배포한다(Apple 계정 없음).
- 기본 설치 경로는 `curl … install.sh | sh` 한 줄이다. dmg는 직접 받으려는 사람을 위해 같이 올린다.

## 실측 사실 (2026-11-11, 이 Mac)

- curl로 받은 파일에는 `com.apple.provenance`만 붙고 `com.apple.quarantine`은 붙지 않았다(확인됨). Gatekeeper 확인 창은 quarantine 표시를 보고 뜨므로, 설치 스크립트로 깐 앱은 확인 창 없이 열린다고 본다(추정: 게시된 릴리스를 새 Mac에서 스크립트로 깔아 열어 보면 확인된다).
- Tauri 2는 `bundle.macOS.signingIdentity`에 `"-"`를 주면 앱을 ad-hoc 서명한다(Tauri 문서 `distribute/sign/macos` "Ad-Hoc Signing"). dmg는 `"-"`일 때 서명하지 않는다(tauri-bundler `dmg/mod.rs`). 같은 문서는 Apple Silicon에서 인터넷에서 받은 앱은 서명이 필요하고, ad-hoc 서명이 없으면 "손상됨"으로 보일 수 있다고 적는다. 로컬 universal 빌드에서 `codesign -dv` = `Signature=adhoc`, `lipo -archs` = `x86_64 arm64`였다.

## 결정

- **ad-hoc 서명만**: `tauri.conf.json` `bundle.macOS.signingIdentity = "-"`. 비밀값(인증서)이 없다.
- **release workflow** (`.github/workflows/release.yml`): `v*` 태그 push 또는 수동 실행. `macos-latest` 한 job에서 `--target universal-apple-darwin --bundles app,dmg`로 빌드하고, `kcal-macos-universal.tar.gz`(.app)·`kcal-macos-universal.dmg`와 각 `.sha256`을 `gh release create --draft`로 초안에 올린다. 빌드 전에 태그가 `v<tauri.conf.json version>`과 같은지, 그 태그의 릴리스가 아직 없는지 검사한다(macOS 분이 Linux의 10배라 먼저 막는다). action은 ci.yml과 같은 SHA이고 새 action은 없다. 토큰은 `GITHUB_TOKEN`만 쓴다(`contents: write`).
- **파일 이름에 버전을 넣지 않는다**: `install.sh`가 `releases/latest/download/<이름>`으로 받기 때문이다. 초안은 `latest`가 아니므로 게시하기 전에는 설치 스크립트가 받지 않는다.
- **tar.gz로 묶는다**: zip(`ditto`)과 달리 `tar -xzf`가 macOS 기본 도구로 바로 풀리고 스크립트가 짧다. `--no-xattrs`로 빌드 머신의 확장 속성을 넣지 않는다. ad-hoc 서명은 확장 속성이 아니라 `Contents/_CodeSignature`와 실행 파일 안에 있어 그대로 남는다.
- **install.sh**: macOS만. 압축과 `.sha256`을 받아 `shasum -a 256 -c`로 확인하고, 틀리면 아무것도 바꾸지 않고 끝낸다. 실행 중인 kcal을 끄고, 새 앱을 설치 폴더 안 임시 이름으로 옮긴 뒤 옛 앱과 바꾼다. `xattr -dr com.apple.quarantine`을 한 번 더 한다(없으면 무시). sudo는 쓰지 않는다. `/Applications`에 쓸 수 없으면 `~/Applications`에 설치한다. 시험용으로 `KCAL_DOWNLOAD_BASE`·`KCAL_INSTALL_DIR`·`KCAL_NO_OPEN`을 받는다.

## 위험

- **Gatekeeper 동작이 바뀔 수 있다**: curl 다운로드에 quarantine이 붙지 않는 것은 2026-11-11 이 Mac에서 본 동작이다. curl이 quarantine을 붙이게 바뀌어도 스크립트가 `xattr -dr`로 지우므로 확인 창은 뜨지 않는다고 본다(추정). Apple이 quarantine 말고 다른 표시(`com.apple.provenance` 등)로 막기 시작하면 스크립트 설치도 "그래도 열기"가 필요해지고, README 안내가 dmg와 같아진다.
- **SHA-256은 무결성만 증명한다**: 체크섬이 같은 릴리스에서 오므로 전송 중 깨짐은 잡지만, 릴리스나 저장소를 바꿀 수 있는 사람이 올린 다른 파일은 막지 못한다. 누가 만들었는지는 증명하지 않는다.
- **`curl | sh`**: 사용자는 main 브랜치의 스크립트를 그대로 실행한다. main에 쓸 수 있는 권한이 곧 설치 권한이다.
- **수동 실행의 태그**: 태그 없이 수동 실행하면 초안을 게시할 때 그 commit에 태그가 생긴다(`--target $GITHUB_SHA`).

## 업그레이드 경로

Apple Developer 계정이 생기면 Developer ID 인증서와 공증 자격 증명을 repository secret으로 넣고 `signingIdentity`를 그 identity로 바꾼다(Tauri bundler가 서명·공증·staple을 한다). 그때 dmg도 서명되고 "그래도 열기" 안내와 이 기록의 위험 첫 줄이 사라진다.

## 남은 것

- Windows 배포는 범위 밖이다.
- 자동 업데이트(Tauri updater)는 없다. 같은 설치 명령을 다시 실행하면 바뀐다.
- release workflow는 GitHub에서 아직 돌려 보지 않았다(태그 push 전).
