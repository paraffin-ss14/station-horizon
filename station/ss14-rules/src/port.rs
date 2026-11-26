//! Adapted SS14 rules. Copyright (c) 2017-2026 Space Wizards Federation.
//! MIT; see ../ss14-port/SS14-LICENSE.txt. Only the documented pure branches.
#![forbid(unsafe_code)]
pub fn can_fit(
    slot: i64,
    clothing: i64,
    has_clothing: bool,
    has_item: bool,
    size: i64,
    small: i64,
) -> Result<bool, String> {
    let (slot, clothing, size, small) = (slot as i32, clothing as i32, size as i32, small as i32);
    let pocket = slot & (1 << 12) == 1 << 12 && has_item && size <= small;
    Ok(!(!has_clothing && !pocket || has_clothing && clothing & slot != slot && !pocket))
}
pub fn slot_action(held: bool, occupied: bool) -> Result<i64, String> {
    Ok(if held && occupied {
        3
    } else if occupied {
        2
    } else if held {
        1
    } else {
        0
    })
}
pub fn priority(
    r#type: i64,
    other_type: i64,
    priority: i64,
    other_priority: i64,
) -> Result<i64, String> {
    let (t, o, p, q) = (
        r#type as i32,
        other_type as i32,
        priority as i32,
        other_priority as i32,
    );
    Ok(if t != o {
        o.wrapping_sub(t)
    } else if p != q {
        q.wrapping_sub(p)
    } else {
        0
    } as i64)
}
pub fn next_hand(current: i64, count: i64, reverse: bool) -> Result<i64, String> {
    let (current, count) = (current as i32, count as i32);
    Ok(if count < 2 {
        current
    } else {
        current
            .wrapping_add(if reverse { -1 } else { 1 })
            .wrapping_add(count)
            % count
    } as i64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pocket_and_clothing() {
        assert_eq!(can_fit(4096, 0, false, true, 1, 1), Ok(true));
        assert_eq!(can_fit(2, 2, true, true, 2, 1), Ok(true));
        assert_eq!(can_fit(2, 0, false, true, 1, 1), Ok(false));
    }
    #[test]
    fn hand_and_priority() {
        assert_eq!(next_hand(0, 2, true), Ok(1));
        assert_eq!(priority(4, 1, 0, 0), Ok(-3));
        assert_eq!(slot_action(true, true), Ok(3));
    }
}
