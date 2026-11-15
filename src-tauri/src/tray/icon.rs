//! 트레이 아이콘 이미지: 에이전트마다 가로 막대 1개(남은 %). 순수 함수만 둔다(docs/decisions/0012).

use crate::usage::Agent;

/// 이미지 모양. 사용자가 조절하는 값이 아니라 플랫폼 크기에 맞춘 상수다(0012).
pub struct Spec {
    pub width: u32,
    pub height: u32,
    pub bar_height: u32,
    pub gap: u32,
    pub margin_x: u32,
    /// true: 검정 + alpha만 그린다(macOS template). false: 에이전트 색 + 회색 트랙.
    pub template: bool,
}

/// macOS: 높이 36px = 18pt(tray-icon이 높이를 18pt로 맞춘다), 가로는 비율대로 24pt.
#[cfg(target_os = "macos")]
pub const SPEC: Spec = Spec {
    width: 48,
    height: 36,
    bar_height: 8,
    gap: 4,
    margin_x: 2,
    template: true,
};

/// Windows/기타: 정사각 32px. 셸이 16px(100%) 또는 32px(200%)로 줄여 쓴다.
#[cfg(not(target_os = "macos"))]
pub const SPEC: Spec = Spec {
    width: 32,
    height: 32,
    bar_height: 7,
    gap: 3,
    margin_x: 2,
    template: false,
};

/// template 트랙의 alpha(채움은 255).
const TEMPLATE_TRACK_ALPHA: u8 = 90;
/// 색 모드의 트랙. 밝은·어두운 작업표시줄 모두에서 보이는 중간 회색.
const TRACK_RGB: [u8; 3] = [128, 128, 128];

/// 에이전트 브랜드색(sRGB). `src/index.css`의 다크 모드 `--agent-*` oklch(0011)를 옮겼다.
/// 다크 값을 쓰는 이유: 작업표시줄은 대체로 어둡고, 밝은 바탕에서도 보이는 밝기다.
/// claude oklch(0.72 0.13 40), codex oklch(0.76 0.15 160), kiro oklch(0.72 0.16 295).
pub fn agent_rgb(agent: Agent) -> [u8; 3] {
    match agent {
        Agent::Claude => [233, 134, 100],
        Agent::Codex => [65, 206, 144],
        Agent::Kiro => [173, 141, 253],
    }
}

/// 남은 %(0~100으로 자른다)를 막대 폭 픽셀로. 반올림한다. 같은 값이면 같은 이미지다.
fn fill_px(percent: f64, track_width: u32) -> u32 {
    (percent.clamp(0.0, 100.0) * track_width as f64 / 100.0).round() as u32
}

fn track_width(spec: &Spec) -> u32 {
    spec.width.saturating_sub(2 * spec.margin_x)
}

/// 막대마다 채움 픽셀 수(값 모름은 `None`). 이미지가 같은지 비교하는 열쇠다.
pub fn quantize(fills: &[Option<f64>], spec: &Spec) -> Vec<Option<u32>> {
    let track = track_width(spec);
    fills.iter().map(|f| f.map(|p| fill_px(p, track))).collect()
}

/// RGBA 이미지 `(픽셀, 가로, 세로)`. 막대는 위에서 아래로 쌓고 세로 가운데에 놓는다.
/// `colors`는 막대별 채움 색이고, template이면 쓰지 않는다. `fills`와 길이가 같아야 한다.
pub fn render(fills: &[Option<f64>], colors: &[[u8; 3]], spec: &Spec) -> (Vec<u8>, u32, u32) {
    let (w, h) = (spec.width, spec.height);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let n = fills.len() as u32;
    let stack = (n * spec.bar_height + n.saturating_sub(1) * spec.gap).min(h);
    let top = (h - stack) / 2;
    let track = track_width(spec);
    for (i, px) in quantize(fills, spec).into_iter().enumerate() {
        let y0 = top + i as u32 * (spec.bar_height + spec.gap);
        let (fill, track_px) = if spec.template {
            ([0, 0, 0, 255], [0, 0, 0, TEMPLATE_TRACK_ALPHA])
        } else {
            let c = colors.get(i).copied().unwrap_or(TRACK_RGB);
            (
                [c[0], c[1], c[2], 255],
                [TRACK_RGB[0], TRACK_RGB[1], TRACK_RGB[2], 255],
            )
        };
        for y in y0..(y0 + spec.bar_height).min(h) {
            for x in 0..track {
                let color = if x < px.unwrap_or(0) { fill } else { track_px };
                let at = ((y * w + spec.margin_x + x) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(&color);
            }
        }
    }
    (rgba, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10x7 이미지, 막대 높이 1, 간격 1, 좌우 여백 1(트랙 8px).
    const SMALL: Spec = Spec {
        width: 10,
        height: 7,
        bar_height: 1,
        gap: 1,
        margin_x: 1,
        template: true,
    };

    /// 채움 '#', 트랙 '.', 투명 ' '.
    fn ascii(image: &(Vec<u8>, u32, u32)) -> String {
        let (rgba, w, _) = image;
        rgba.chunks(4 * *w as usize)
            .map(|row| {
                row.chunks(4)
                    .map(|p| match p[3] {
                        0 => ' ',
                        TEMPLATE_TRACK_ALPHA => '.',
                        _ => '#',
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn draw(fills: &[Option<f64>]) -> String {
        ascii(&render(fills, &[[0; 3]; 3], &SMALL))
    }

    #[test]
    fn draws_three_bars_at_100_50_and_0_percent() {
        let expected = [
            "          ",
            " ######## ",
            "          ",
            " ####.... ",
            "          ",
            " ........ ",
            "          ",
        ]
        .join("\n");
        assert_eq!(draw(&[Some(100.0), Some(50.0), Some(0.0)]), expected);
    }

    #[test]
    fn draws_only_the_track_for_unknown_values() {
        let expected = [
            "          ",
            " ######## ",
            "          ",
            " ........ ",
            "          ",
            " ##...... ",
            "          ",
        ]
        .join("\n");
        assert_eq!(draw(&[Some(100.0), None, Some(25.0)]), expected);
    }

    #[test]
    fn centers_a_single_bar_vertically() {
        let expected = [
            "          ",
            "          ",
            "          ",
            " ###..... ",
            "          ",
            "          ",
            "          ",
        ]
        .join("\n");
        assert_eq!(draw(&[Some(37.5)]), expected);
    }

    #[test]
    fn clamps_values_outside_0_to_100() {
        assert_eq!(
            quantize(&[Some(-5.0), Some(130.0)], &SMALL),
            [Some(0), Some(8)]
        );
    }

    #[test]
    fn rounds_fill_to_the_nearest_pixel() {
        // 8px 트랙에서 1px = 12.5%. 6.25%가 경계(0.5px)로 올림된다.
        let px = |p| quantize(&[Some(p)], &SMALL)[0];
        assert_eq!(px(6.2), Some(0));
        assert_eq!(px(6.25), Some(1));
        assert_eq!(px(18.7), Some(1));
        assert_eq!(px(18.75), Some(2));
    }

    #[test]
    fn paints_agent_color_and_gray_track_when_not_template() {
        let spec = Spec {
            template: false,
            ..SMALL
        };
        let colors = [agent_rgb(Agent::Claude)];
        let (rgba, w, _) = render(&[Some(50.0)], &colors, &spec);
        let at = |x: u32| {
            let i = ((3 * w + x) * 4) as usize; // 막대는 3번째 줄
            rgba[i..i + 4].to_vec()
        };
        assert_eq!(at(1), [233, 134, 100, 255]);
        assert_eq!(at(8), [128, 128, 128, 255]);
        assert_eq!(at(0), [0, 0, 0, 0]);
    }

    /// 실제 크기 확인용. `cargo test -- --ignored print_real_size --nocapture`
    #[test]
    #[ignore]
    fn print_real_size() {
        let fills = [Some(100.0), Some(62.0), None];
        let image = render(&fills, &[[0; 3]; 3], &super::SPEC);
        let (rgba, w, h) = &image;
        println!("spec {w}x{h}, template={}", super::SPEC.template);
        let line = |row: &[u8]| {
            row.chunks(4)
                .map(|p| match p[3] {
                    0 => ' ',
                    255 => '#',
                    _ => '.',
                })
                .collect::<String>()
        };
        for row in rgba.chunks(4 * *w as usize) {
            println!("|{}|", line(row));
        }
    }
}
