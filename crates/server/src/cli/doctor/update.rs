//! 진단(65차): 새 판 — red 뼈대.

use std::path::Path;

use super::{Check, CheckItem, good};

/// 진단: 새 판(red 뼈대).
pub(super) fn check_update(home: &Path) -> Check {
    let _ = home;
    good(CheckItem::Update, "")
}
