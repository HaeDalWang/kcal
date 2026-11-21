# 0029. 서명 없는 Windows x64 릴리스

## Summary (English)

- Decision: publish an unsigned Windows x64 NSIS installer as `kcal-windows-x64-setup.exe`, with a SHA-256 file, beside the existing macOS artifacts. No ARM64 or MSI installer.
- Why: the user decided to publish Windows on 2026-11-21; a Windows colleague will test the installer. There is no Windows signing certificate.
- Release: the Windows job needs the macOS job and receives its resolved tag through a job output. Only macOS creates the draft; Windows uploads to it. Publication remains manual.
- Risks: SmartScreen can show “Windows protected your PC”; users may need More info → Run anyway. Organization policy can prevent that. SHA-256 checks integrity, not authorship.
- Untested: the release workflow and NSIS install/uninstall, npm `claude.cmd` execution, light mode, and popover placement across monitors with different scaling need Windows validation.

- 상태: 결정(구현함). 릴리스 게시는 사람이 한다.
- 날짜: 2026-11-21
- 앞선 결정: [0020](0020-unsigned-macos-release.md) 서명 없는 macOS 릴리스, [0001](0001-initial-stack.md) 스택

## 문제

Windows 빌드는 있지만 공개 설치 파일은 없다. Windows 사용자가 받아 설치하고, 읽는 파일·설정 위치·트레이·자동 시작·삭제 방법을 확인할 수 있어야 한다.

## 사용자 결정 (2026-11-21)

- Windows를 공개한다. Windows 동료가 설치 파일을 시험한다.
- 기존 macOS 산출물과 고정 이름, capabilities·CSP·mcp feature 경계는 유지한다.

## 결정

- **서명 없는 NSIS x64만** 배포한다. 새 의존성·action·서명 비밀값을 추가하지 않는다. SmartScreen 경고와 실행 방법을 README 두 언어에 적는다. ARM64·MSI·자동 업데이트는 이번 범위 밖이다.
- **초안은 하나**: `windows-latest` job은 `needs: macos`로 기다린 뒤 macOS job output의 tag를 받아 `gh release upload`한다. macOS만 `gh release create --draft`한다. Windows 실패 시 macOS 파일만 든 초안이 남으므로 두 플랫폼 파일을 확인한 뒤 사람이 게시한다.
- **고정 이름·체크섬**: `kcal-windows-x64-setup.exe`와 `.exe.sha256`을 올린다. 체크섬은 SHA-256 소문자 hex + 공백 두 개 + 파일 이름이다. 기존 macOS 이름은 바꾸지 않는다. 기존 pinned SHA의 action만 재사용하고 `GH_TOKEN`은 Windows 업로드 step에만 둔다. Windows runner 비용은 Linux의 2배(macOS는 10배)다.
- **Windows 경로 보완**: Claude CLI는 native 설치 위치, `%APPDATA%\npm\claude.cmd`, PATH 순으로 찾는다. 기존 인자 배열을 받는 Command 실행기를 그대로 쓴다. `cmd /c` 문자열을 만들지 않는다. Keychain timeout은 macOS에서만 표시한다. 시작 컨텍스트의 `\\?\`·`\\?\UNC\`는 표시 문자열에서만 제거하고 실제 경로·비교는 유지한다.
- **설정 위치 근거**: `settings.rs::from_app`는 `app_config_dir()/settings.json`을 쓴다. Tauri의 `app_config_dir`는 `dirs::config_dir()/identifier`, `app_data_dir`는 `dirs::data_dir()/identifier`다. Windows의 두 dirs 함수는 roaming app data를 사용한다(로컬 캐시의 Tauri 2.12.5 `src/path/desktop.rs`, dirs 7.0.0 `src/win.rs` 확인). identifier는 `io.github.haedalwang.kcal`이므로 `%APPDATA%\io.github.haedalwang.kcal`에 저장한다.

## 위험

- 서명이 없어 제작자를 인증서로 증명하지 못한다. SmartScreen에서 실행을 막을 수 있으며 조직 정책에서는 우회 선택 자체가 없을 수 있다.
- macOS 빌드가 실패하면 Windows job도 실행되지 않는다. 초안 생성 주체를 하나로 유지하기 위한 의도된 순서다.
- 체크섬은 같은 릴리스에서 제공하므로 전송 오류만 확인하고 제작자는 증명하지 않는다.

## 남은 것

- GitHub release workflow 실행과 실제 Windows NSIS 설치·제거·업데이트 확인.
- npm `claude.cmd` 실행과 경로·권한 실기 확인. 후보 생성은 호스트 독립 단위 테스트로 확인한다.
- 라이트 모드, 배율이 다른 다중 모니터 팝오버 위치. 개발 빌드의 트레이·툴팁·팝오버는 기존 Windows 11 PC 한 대에서 확인한 범위다.
- 인증서를 마련하면 Windows code signing을 별도 결정으로 추가한다.
