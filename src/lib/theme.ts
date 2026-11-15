// 테마는 OS를 따른다. 첫 렌더 전 적용은 public/theme-init.js가 맡는다. 규칙을 바꾸면 그 파일도 같이 고친다.
export const DARK_QUERY = "(prefers-color-scheme: dark)";

function apply(isDark: boolean): void {
  document.documentElement.classList.toggle("dark", isDark);
}

/** `dark` class를 OS 설정에 맞추고, 설정이 바뀌면 다시 맞춘다. 해제 함수를 돌려준다. */
export function followSystemTheme(): () => void {
  const media = window.matchMedia(DARK_QUERY);
  const onChange = () => apply(media.matches);
  onChange();
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}
