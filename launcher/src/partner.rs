//! Compatibility surface for retained legacy launcher code. Sponsor artwork and ads are removed.
use crate::theme::Theme;
use gpui::{div, prelude::*, Div, Stateful};

pub const SEEN_BEFORE_HIDE: u64 = 0;
pub const TALL_HEIGHT: f32 = 0.;

pub fn hide_button(id: &'static str, _theme: &Theme) -> Stateful<Div> {
    div().id(id)
}
pub fn banner(id: &'static str, _width: f32, _theme: &Theme) -> Stateful<Div> {
    div().id(id)
}
pub fn banner_image(id: &'static str, _width: f32, _theme: &Theme) -> Stateful<Div> {
    div().id(id)
}
pub fn card(id: &'static str, _width: f32, _compact: bool, _theme: &Theme) -> Stateful<Div> {
    div().id(id)
}
