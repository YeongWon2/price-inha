//! 순위 셈: 최신성 감쇠와 MMR. 모두 백만분의 1 정수로 셈한다(실수를 쓰지 않는다).

use std::collections::HashSet;

const ONE: i128 = 1_000_000;
const DAY_MS: i128 = 86_400_000;

/// 최신성 감쇠 계수(백만분의 1). 반감기마다 반으로 준다: `2^(-나이/반감기)`.
/// 반감기가 0이거나 나이가 0 이하이면 1_000_000(감쇠 없음).
pub(crate) fn decay_micros(age_ms: i64, half_life_days: u32) -> i64 {
    if half_life_days == 0 || age_ms <= 0 {
        return 1_000_000;
    }
    let half_ms = i128::from(half_life_days) * DAY_MS;
    let age = i128::from(age_ms);
    let whole = age / half_ms;
    if whole >= 40 {
        return 0;
    }
    let fraction = (age % half_ms) * ONE / half_ms;
    let factor = pow2_neg(fraction) >> whole;
    i64::try_from(factor).unwrap_or(0)
}

/// `2^(-r)`(r은 0 이상 1 미만, 백만분의 1)을 `e^(-r ln 2)`의 앞 여섯 항으로 어림한다.
fn pow2_neg(r: i128) -> i128 {
    // (ln 2)^k / k! 의 백만분의 1.
    const COEFFS: [i128; 6] = [693_147, 240_227, 55_504, 9_618, 1_333, 154];
    let mut power = ONE;
    let mut sum = ONE;
    for (k, coeff) in COEFFS.iter().enumerate() {
        power = power * r / ONE;
        let term = coeff * power / ONE;
        if k % 2 == 0 {
            sum -= term;
        } else {
            sum += term;
        }
    }
    sum.max(0)
}

/// MMR 후보: 관련도(백만분의 1)와 글의 낱말 집합.
pub(crate) struct Candidate {
    pub(crate) rank: i64,
    pub(crate) tokens: HashSet<String>,
}

/// 글을 낱말 집합으로 쪼갠다(글자와 숫자가 아닌 것에서 끊고 소문자로).
pub(crate) fn token_set(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// 두 낱말 집합의 비슷함(자카드, 백만분의 1).
fn similarity_micros(a: &HashSet<String>, b: &HashSet<String>) -> i128 {
    let inter = i128::try_from(a.intersection(b).count()).unwrap_or(0);
    let union = i128::try_from(a.union(b).count()).unwrap_or(0);
    if union == 0 { 0 } else { inter * ONE / union }
}

/// MMR로 `limit`개를 고른다. 후보는 관련도 순서(같으면 앞선 것이 먼저)로 와야 하고, 돌려주는 것은
/// 고른 순서의 위치다. 관련도는 최소~최대를 0~1로 다시 편다. `lambda`가 1_000_000이면 점수순 그대로.
pub(crate) fn mmr_select(candidates: &[Candidate], limit: usize, lambda_micros: i64) -> Vec<usize> {
    let lambda = i128::from(lambda_micros.clamp(0, 1_000_000));
    let low = candidates
        .iter()
        .map(|c| i128::from(c.rank))
        .min()
        .unwrap_or(0);
    let high = candidates
        .iter()
        .map(|c| i128::from(c.rank))
        .max()
        .unwrap_or(0);
    let relevance = |c: &Candidate| -> i128 {
        if high == low {
            ONE
        } else {
            (i128::from(c.rank) - low) * ONE / (high - low)
        }
    };
    let mut remaining: Vec<usize> = (0..candidates.len()).collect();
    let mut chosen: Vec<usize> = Vec::new();
    while chosen.len() < limit && !remaining.is_empty() {
        let mut best: Option<(usize, i128)> = None;
        for (slot, &index) in remaining.iter().enumerate() {
            let candidate = &candidates[index];
            let nearest = chosen
                .iter()
                .map(|&done| similarity_micros(&candidate.tokens, &candidates[done].tokens))
                .max()
                .unwrap_or(0);
            let value = (lambda * relevance(candidate) - (ONE - lambda) * nearest) / ONE;
            if best.is_none_or(|(_, top)| value > top) {
                best = Some((slot, value));
            }
        }
        if let Some((slot, _)) = best {
            chosen.push(remaining.remove(slot));
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_halves_every_half_life() {
        let day = 86_400_000;
        assert_eq!(decay_micros(0, 30), 1_000_000);
        assert_eq!(decay_micros(30 * day, 30), 500_000);
        assert_eq!(decay_micros(60 * day, 30), 250_000);
        let mid = decay_micros(15 * day, 30);
        assert!((mid - 707_107).abs() < 1_000, "{mid}");
        assert_eq!(decay_micros(5 * day, 0), 1_000_000);
    }

    #[test]
    fn mmr_prefers_a_different_candidate() {
        let set = |s: &str| token_set(s);
        let candidates = vec![
            Candidate {
                rank: 100,
                tokens: set("a b c"),
            },
            Candidate {
                rank: 100,
                tokens: set("a b c"),
            },
            Candidate {
                rank: 50,
                tokens: set("x y z"),
            },
        ];
        assert_eq!(mmr_select(&candidates, 2, 1_000_000), vec![0, 1]);
        assert_eq!(mmr_select(&candidates, 2, 300_000), vec![0, 2]);
    }
}
