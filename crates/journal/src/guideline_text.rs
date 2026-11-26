//! (결정 30) 지침 글 다루기: 1층 내용 거르기 · 글의 비슷함 · 글자 상한 자르기.

/// 글이 1층(한도 · 금지 · 승인 길)을 바꾸려는가: 숫자 한도(숫자 뒤 `%` · `원` · `주` · `배` · `퍼센트` · `만` · `억`),
/// 종목 지정(여섯 자리 이상 숫자), 한도 · 금지 · 승인 우회 낱말. 이런 글은 지침이 되지 못하고 1층 개선안 길로만 간다.
pub fn touches_layer_one(text: &str) -> bool {
    if LAYER_ONE_WORDS.iter().any(|word| text.contains(word)) {
        return true;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut run = 0_usize;
    for (at, ch) in chars.iter().enumerate() {
        if !ch.is_ascii_digit() {
            run = 0;
            continue;
        }
        run += 1;
        if run >= 6 {
            return true;
        }
        let next = chars[at + 1..].iter().find(|c| !c.is_whitespace());
        let unit_follows = next.is_some_and(|c| !c.is_ascii_digit() && LIMIT_UNITS.contains(c))
            || chars[at + 1..]
                .iter()
                .collect::<String>()
                .trim_start()
                .starts_with("퍼센트");
        if unit_follows {
            return true;
        }
    }
    false
}

/// 한도 · 금지 · 승인 길을 바꾸려는 낱말.
const LAYER_ONE_WORDS: [&str; 10] = [
    "한도",
    "상한",
    "하한",
    "금지",
    "블랙리스트",
    "승인 없이",
    "승인없이",
    "승인을 건너",
    "승인 생략",
    "규칙을 바꾸",
];

/// 숫자 뒤에 오면 숫자 한도로 보는 단위.
const LIMIT_UNITS: [char; 7] = ['%', '％', '원', '주', '배', '만', '억'];

/// 두 글의 비슷함(백분율): 공백을 뺀 글자 두 개 묶음의 겹침(자카드). 둘 다 짧아 묶음이 없으면 같은 글일 때만 100.
pub fn similarity_pct(a: &str, b: &str) -> u32 {
    let (left, right) = (bigrams(a), bigrams(b));
    if left.is_empty() || right.is_empty() {
        let squeeze = |text: &str| {
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        };
        return if squeeze(a) == squeeze(b) { 100 } else { 0 };
    }
    let shared = left.intersection(&right).count();
    let all = left.union(&right).count();
    u32::try_from(shared * 100 / all.max(1)).unwrap_or(0)
}

/// 공백을 뺀 글자 두 개 묶음들.
fn bigrams(text: &str) -> std::collections::HashSet<(char, char)> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    chars.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

/// 글을 글자 상한에서 자른다.
pub(crate) fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// 글들을 비슷함으로 묶는다(묶음의 어느 글과라도 `pct` 이상이면 그 묶음). 묶음은 처음 나온 차례이고, 안은 받은
/// 차례(번호)다.
pub(crate) fn group_similar(texts: &[&str], pct: u32) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (at, text) in texts.iter().enumerate() {
        let home = groups
            .iter()
            .position(|group| group.iter().any(|&i| similarity_pct(texts[i], text) >= pct));
        match home {
            Some(found) => groups[found].push(at),
            None => groups.push(vec![at]),
        }
    }
    groups
}
