//! 줄 단위 차이(LCS, 새 의존성 없음). Claude에게 맡긴 결과 미리보기(0018 작업 A)에 쓴다.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffKind {
    Same,
    Removed,
    Added,
}

/// 차이 한 줄. TS 짝은 `ipc.ts` `DiffLine`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// LCS 표 칸 수 상한(약 16MB). 넘으면 원본 전체를 빼고 결과 전체를 넣은 것으로 보인다.
// ponytail: O(n·m) 표. 2,000줄 × 2,000줄까지. 더 긴 안내문이 흔해지면 Myers diff로 바꾼다.
const MAX_CELLS: usize = 4_000_000;

fn split(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn line(kind: DiffKind, text: &str) -> DiffLine {
    DiffLine {
        kind,
        text: text.to_owned(),
    }
}

/// 뒤에서부터 채운 LCS 길이 표. `table[i][j]` = `a[i..]`와 `b[j..]`의 LCS 길이.
fn lcs_table(a: &[&str], b: &[&str]) -> Vec<Vec<u32>> {
    let mut table = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table
}

pub fn lines(before: &str, after: &str) -> Vec<DiffLine> {
    let (a, b) = (split(before), split(after));
    if a.len().saturating_mul(b.len()) > MAX_CELLS {
        return a
            .iter()
            .map(|t| line(DiffKind::Removed, t))
            .chain(b.iter().map(|t| line(DiffKind::Added, t)))
            .collect();
    }
    let table = lcs_table(&a, &b);
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            out.push(line(DiffKind::Same, a[i]));
            (i, j) = (i + 1, j + 1);
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push(line(DiffKind::Removed, a[i]));
            i += 1;
        } else {
            out.push(line(DiffKind::Added, b[j]));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|t| line(DiffKind::Removed, t)));
    out.extend(b[j..].iter().map(|t| line(DiffKind::Added, t)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(diff: &[DiffLine]) -> Vec<(DiffKind, &str)> {
        diff.iter().map(|l| (l.kind, l.text.as_str())).collect()
    }

    #[test]
    fn keeps_common_lines_and_marks_removed_and_added_ones() {
        let diff = lines("a\nb\nc\nd\n", "a\nc\nx\nd");
        assert_eq!(
            shape(&diff),
            [
                (DiffKind::Same, "a"),
                (DiffKind::Removed, "b"),
                (DiffKind::Same, "c"),
                (DiffKind::Added, "x"),
                (DiffKind::Same, "d"),
            ]
        );
    }

    #[test]
    fn empty_result_removes_every_line() {
        let diff = lines("a\nb", "");
        assert_eq!(
            shape(&diff),
            [(DiffKind::Removed, "a"), (DiffKind::Removed, "b")]
        );
    }
}
