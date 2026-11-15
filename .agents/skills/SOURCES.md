# Skills 출처

모든 Skill은 이 레포 로컬 범위다. 전역 설치(~/.claude/skills, ~/.agents/skills)는 사용하지 않는다.
`.claude/skills`는 이 디렉터리를 가리키는 symlink다. Codex는 `.agents/skills`를 직접 읽는다.
설치일: 2026-10-28. 업데이트는 아래 commit에서 diff를 검토한 뒤 디렉터리째 교체한다.

| Skill | 출처 (commit) | 라이선스 | 외부 명령/네트워크 |
|---|---|---|---|
| tauri-architecture | github.com/fxckcode/tauri-skills @ d3959ff | MIT | 없음 (문서만) |
| tauri-rust | 〃 | MIT | 없음 |
| tauri-security | 〃 | MIT | 없음 |
| tauri-frontend | 〃 | MIT | 없음 |
| tauri-testing | 〃 | MIT | 없음 |
| react-best-practices (name: vercel-react-best-practices) | github.com/vercel-labs/agent-skills @ 1a2c79d | MIT | 없음 (코드 예시의 fetch만) |
| shadcn | github.com/shadcn-ui/ui @ 96da9cc (skills/shadcn) | MIT | `allowed-tools`: `pnpm dlx shadcn@latest *` 등. shadcn registry에서 컴포넌트를 내려받는다. `scripts/showcase-coverage.mjs`는 로컬 분석 스크립트 |

## 제외한 것

- tauri-mobile, tauri-release, tauri-plugins, tauri-setup: 범위 밖(모바일·배포)이거나 필수 범위가 아니다. 필요할 때 추가한다.
- web-design-guidelines: 실행할 때마다 raw.githubusercontent.com에서 규칙을 WebFetch한다(고정되지 않은 원격 내용).
- composition-patterns, react-native-skills, react-view-transitions, deploy-to-vercel, vercel-*: 범위 밖이다.

전체 commit: tauri-skills d3959ffae74a04ae0ae8e4ad577e72b66c5e02d4, agent-skills 1a2c79d4e6c4a2283dc2c614b6538cbe1ed5ca62, shadcn-ui/ui 96da9cc7f1aa10580a3741664e2a77c721206c00
