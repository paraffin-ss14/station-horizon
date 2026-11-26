//! Extracted SS14 UI branches. Copyright (c) 2017-2026 Space Wizards Federation. MIT.
#![forbid(unsafe_code)]
pub fn show_category(count: i64, extra: bool) -> Result<bool, String> {
    Ok(count != 0 || extra)
}
pub fn tooltip_text(has_message: bool, message: String, text: String) -> Result<String, String> {
    Ok(if has_message { message } else { text })
}
