// 첫 렌더 전에 OS 테마 class를 붙여 깜빡임을 막는다.
// CSP(script-src 'self')를 지키기 위해 inline script 대신 별도 파일로 둔다. 규칙은 src/lib/theme.ts와 같다.
// macOS(popover)·Windows(acrylic)는 창 자체가 반투명이라 배경을 투명하게 둔다(index.css `.vibrancy`). 다른 OS는 불투명 배경.
(function () {
  var root = document.documentElement;
  root.classList.toggle("dark", window.matchMedia("(prefers-color-scheme: dark)").matches);
  root.classList.toggle("vibrancy", /Mac|Windows/.test(navigator.userAgent));
})();
