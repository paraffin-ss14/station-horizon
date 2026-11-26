//! Copyright (c) 2017-2026 Space Wizards Federation. MIT: ../ss14-port/SS14-LICENSE.txt.
#![forbid(unsafe_code)]
pub fn tags_allowed(
    tags: i64,
    denied: i64,
    first: i64,
    second: i64,
    groups: i64,
) -> Result<bool, String> {
    let (tags, denied, first, second, groups) = (
        tags as u8,
        denied as u8,
        first as u8,
        second as u8,
        groups as i32,
    );
    Ok(tags & denied == 0
        && (groups <= 0 || first & tags == first || (groups > 1 && second & tags == second)))
}
