# 0002. 스캐폴딩 세부 결정

- 상태: 채택
- 날짜: 2026-10-28

## 의존성

| 결정                                                | 이유                                                                                                                                                                                                                                                                                                                              |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| shadcn style `base-nova`(Base UI, `@base-ui/react`) | shadcn CLI 4.23.0 기본값. radix보다 나을 근거가 없어 기본을 따른다                                                                                                                                                                                                                                                                |
| `cn`, `class-variance-authority` 유지               | shadcn 컴포넌트가 직접 import한다                                                                                                                                                                                                                                                                                                 |
| `@fontsource-variable/geist` 제거                   | 요구 사항이 system-ui 폰트 스택이다. 웹폰트를 번들에 넣지 않는다                                                                                                                                                                                                                                                                  |
| `tw-animate-css` 제거                               | 쓰는 컴포넌트(button, card, input, badge, separator)에 애니메이션 클래스가 없다. overlay 컴포넌트를 추가할 때 다시 넣는다                                                                                                                                                                                                         |
| `shadcn`을 devDependency 4.22.1로 고정              | `src/index.css`의 `@import "shadcn/tailwind.css"`(custom variant 정의)에만 쓴다. 빌드 시점에만 필요하다. 4.23.0은 공개 1일 미만이라 pnpm `minimumReleaseAge` 정책에 걸려 예외 등록이 필요했다. 4.22.1과 4.23.0의 `dist/tailwind.css`가 바이트 단위로 같음을 확인하고 예외 없이 4.22.1을 쓴다. CLI 실행은 `pnpm dlx shadcn@4.23.0` |
| `lucide-react` ~1.61.0                              | 1.62.0도 같은 정책에 걸렸다. 정책을 완화하지 않고 바로 이전 버전을 쓴다                                                                                                                                                                                                                                                           |
| `@testing-library/jest-dom` 미사용                  | 기본 assertion으로 충분하다                                                                                                                                                                                                                                                                                                       |
| Rust `thiserror`                                    | `AppError`의 Display 구현용. tauri가 이미 의존하는 crate다                                                                                                                                                                                                                                                                        |

## CSP

- `app.security.csp`는 `'self'` 위주로 최소화했다. Tauri가 빌드 시 inline script hash와 style nonce를 자동 추가한다.
- 첫 렌더 테마 적용은 inline script 대신 `public/theme-init.js`(동기 classic script)로 했다. `script-src 'self'`로 허용되므로 hash를 손으로 관리하지 않는다.
- macOS 데스크톱 `tauri dev`는 WebView가 Vite dev 서버(`http://127.0.0.1:1420`)를 직접 열어 CSP 헤더가 붙지 않는다(2026-10-28 확인: 응답 헤더 없음, 위반 이벤트 없음). `devCsp`는 dev 서버를 proxy하는 플랫폼을 위한 완화 정책(Vite HMR용 inline script/style, ws)이며 macOS에서는 효과가 없다. Windows는 미검증이다.

## MCP bridge와 CSP

- bridge는 helper script를 페이지에 주입한다. CSP가 켜진 번들 빌드에서는 이 주입이 막혀 `webview_execute_js`, 스크린샷 등이 실패한다("Resolve-ref helper was not available"). MCP는 CSP가 없는 `pnpm tauri:dev:mcp`에서만 쓴다.
- 기본 capability에는 bridge 권한이 없다. `tauri.mcp.conf.json` overlay가 `app.security.capabilities`에 `"default"`와 inline capability `mcp-dev`(`mcp-bridge:default`)를 함께 지정한다. 배열을 지정하면 나열된 capability만 활성화되므로 `"default"`를 반드시 같이 적는다.
- 바인드 주소는 `127.0.0.1`로 고정했다(`lsof`로 확인).
