//! 진단(65차): 새 판. 서버가 남긴 확인 기록(`update-check.json`)만 읽는다 — 바깥에 묻지 않고 아무것도 받지 않는다.
//! 늘 문제가 아니다(알리기만).

use std::path::Path;

use chrono::{FixedOffset, TimeZone};

use super::{Check, CheckItem, good};
use crate::cli::VERSION;
use crate::update_check::read_update_notice;

/// 진단: 새 판이 있으면 그 판과 바꾸는 길(설치 한 줄을 다시)을 알린다.
pub(super) fn check_update(home: &Path) -> Check {
    let item = CheckItem::Update;
    let Some(notice) = read_update_notice(home) else {
        return good(
            item,
            format!(
                "새 판 확인 기록이 없습니다(지금 v{VERSION}. 서버가 하루 한 번 확인하고, 시연 모드는 확인하지 않습니다)"
            ),
        );
    };
    let when = kst_day(notice.checked_at);
    if notice.newer {
        good(
            item,
            format!(
                "새 판 {}이 있습니다(지금 v{}, {when} 확인). 바꾸려면 설치 한 줄(install.sh · install.ps1)을 다시 \
돌립니다 — 저절로 내려받거나 바꾸지 않습니다",
                notice.latest, notice.current
            ),
        )
    } else {
        good(
            item,
            format!("최신 판입니다(v{}, {when} 확인)", notice.current),
        )
    }
}

/// 한국 날짜 `YYYY-MM-DD`.
fn kst_day(at: i64) -> String {
    FixedOffset::east_opt(9 * 3600)
        .and_then(|kst| kst.timestamp_millis_opt(at).single())
        .map_or_else(|| "?".to_string(), |t| t.format("%Y-%m-%d").to_string())
}
