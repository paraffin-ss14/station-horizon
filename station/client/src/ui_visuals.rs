//! Native station UI skin; only the attributed RSI sprites are reused from SS14.
use crate::ui::{Ui, tr};
use egui::{Color32, RichText, Stroke};
#[path = "../../ss14-ui-rules/src/port.rs"]
mod rules;
pub const ACCENT: Color32 = Color32::from_rgb(170, 190, 210);
const BACKGROUND: Color32 = Color32::from_rgb(28, 29, 33);
const SURFACE: Color32 = Color32::from_rgb(42, 43, 48);
const LINE: Color32 = Color32::from_rgb(96, 99, 108);
pub fn theme(ctx: &egui::Context) {
    let id = egui::Id::unique("station-ui-theme");
    if ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, true));
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = egui::vec2(3., 3.);
        s.spacing.button_padding = egui::vec2(5., 3.);
        s.visuals = egui::Visuals::dark();
        s.visuals.widgets.noninteractive.fg_stroke =
            Stroke::new(1., Color32::from_rgb(215, 225, 233));
        s.visuals.panel_fill = BACKGROUND;
        s.visuals.window_fill = SURFACE;
        s.visuals.window_stroke = Stroke::new(1., LINE);
        s.visuals.window_corner_radius = 0.into();
        for w in [
            &mut s.visuals.widgets.inactive,
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
            &mut s.visuals.widgets.noninteractive,
        ] {
            w.corner_radius = 0.into();
        }
        s.visuals.extreme_bg_color = BACKGROUND;
        s.visuals.selection.bg_fill = Color32::from_rgb(65, 70, 80);
        s.visuals.selection.stroke = Stroke::new(1., ACCENT);
        s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(53, 54, 61);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(53, 54, 61);
        s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(71, 74, 84);
        s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(71, 74, 84);
        s.visuals.widgets.hovered.bg_stroke = Stroke::new(1., ACCENT);
        s.visuals.widgets.active.bg_fill = Color32::from_rgb(65, 80, 87);
    });
}
pub fn show_category(count: usize) -> bool {
    rules::show_category(count as i64, false).unwrap_or(false)
}
pub fn tooltip(reason: &str, label: &str) -> String {
    rules::tooltip_text(!reason.is_empty(), tr(reason).into(), tr(label).into()).unwrap_or_default()
}
pub fn portrait(ui: &mut egui::Ui, texture: Option<egui::TextureId>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3., BACKGROUND);
    if let Some(texture) = texture {
        ui.painter().image(
            texture,
            rect.shrink(4.),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            Color32::WHITE,
        );
    }
}
pub fn item_image(ui: &mut egui::Ui, icon: Option<crate::sprites::Icon>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if let Some((texture, uv, color)) = icon {
        let c = color.map(|v| (v.clamp(0., 1.) * 255.) as u8);
        ui.painter().image(
            texture,
            rect,
            uv,
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
        );
    }
}
pub fn equipment_slot(
    ui: &mut egui::Ui,
    label: &str,
    icon: Option<crate::sprites::Icon>,
    slot: usize,
    empty: egui::TextureId,
) -> egui::Response {
    let response = ui.add_sized(
        [64., 64.],
        egui::Button::new("").sense(egui::Sense::click_and_drag()),
    );
    let rect = response.rect;
    let uv = egui::Rect::from_min_max(
        egui::pos2(slot as f32 / 18., 0.),
        egui::pos2((slot + 1) as f32 / 18., 1.),
    );
    let (texture, uv, color) = icon.unwrap_or((empty, uv, [0.55, 0.65, 0.73, 0.7]));
    let c = color.map(|v| (v.clamp(0., 1.) * 255.) as u8);
    let image = egui::Rect::from_center_size(rect.center(), egui::vec2(56., 56.));
    ui.painter().rect_stroke(
        rect.shrink(1.),
        0.,
        Stroke::new(1., Color32::from_rgb(103, 105, 114)),
        egui::StrokeKind::Inside,
    );
    ui.painter().line_segment(
        [
            rect.left_bottom() - egui::vec2(-2., 2.),
            rect.right_bottom() - egui::vec2(2., 2.),
        ],
        Stroke::new(2., Color32::from_rgb(24, 24, 28)),
    );
    ui.painter().image(
        texture,
        image,
        uv,
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    response.on_hover_text(label)
}
impl Ui {
    pub fn lobby(&mut self, root: &mut egui::Ui, f: &station_server::Frame) {
        use station_server::{Job, RoundCommand};
        let Some(round) = f.round else {
            return;
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(24))
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "HORIZON  /  {} {:03}",
                            tr("round.number"),
                            round.number
                        ))
                        .color(ACCENT)
                        .size(13.),
                    );
                    ui.add_space(12.);
                    ui.heading(RichText::new(tr("lobby.title")).size(30.));
                    ui.label(
                        RichText::new(tr("lobby.subtitle")).color(Color32::from_rgb(160, 180, 194)),
                    );
                    ui.add_space(20.);
                    if let Some(crew) = &f.crew {
                        let wide = ui.available_width() >= 700.;
                        let preview = |ui: &mut egui::Ui| {
                            ui.vertical(|ui| {
                                if wide {
                                    ui.set_width(216.);
                                }
                                egui::Frame::NONE
                                    .fill(SURFACE)
                                    .stroke(Stroke::new(1., LINE))
                                    .inner_margin(18)
                                    .show(ui, |ui| {
                                        ui.set_min_width(if wide { 180. } else { 100. });
                                        ui.label(RichText::new(tr("lobby.crew")).color(ACCENT));
                                        portrait(
                                            ui,
                                            self.portraits.get(crew.selected.index()).copied(),
                                            if wide { 160. } else { 80. },
                                        );
                                        ui.heading(tr(crew.selected.key()));
                                        ui.label(tr(if round.ready {
                                            "lobby.ready_status"
                                        } else {
                                            "lobby.select_status"
                                        }));
                                    });
                            });
                        };
                        let portraits = self.portraits.clone();
                        let mut jobs = |ui: &mut egui::Ui| {
                            ui.heading(tr("lobby.choose"));
                            ui.add_space(6.);
                            for job in Job::ALL {
                                let selected = crew.selected == job;
                                let enabled = selected || crew.vacancies[job.index()] > 0;
                                let width = ui.available_width().min(520.);
                                let response = ui.add_enabled(
                                    enabled,
                                    egui::Button::new("")
                                        .selected(selected)
                                        .min_size(egui::vec2(width.max(240.), 78.)),
                                );
                                let rect = response.rect;
                                if let Some(texture) = portraits.get(job.index()) {
                                    ui.painter().image(
                                        *texture,
                                        egui::Rect::from_min_size(
                                            rect.min + egui::vec2(10., 9.),
                                            egui::vec2(60., 60.),
                                        ),
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1., 1.),
                                        ),
                                        Color32::WHITE,
                                    );
                                }
                                let text_color = if enabled {
                                    Color32::from_rgb(223, 232, 238)
                                } else {
                                    Color32::GRAY
                                };
                                ui.painter().text(
                                    rect.min + egui::vec2(84., 16.),
                                    egui::Align2::LEFT_TOP,
                                    tr(job.key()),
                                    egui::FontId::proportional(18.),
                                    text_color,
                                );
                                ui.painter().text(
                                    rect.min + egui::vec2(84., 44.),
                                    egui::Align2::LEFT_TOP,
                                    format!(
                                        "{}: {}",
                                        tr("lobby.vacancies"),
                                        crew.vacancies[job.index()]
                                    ),
                                    egui::FontId::proportional(12.),
                                    text_color,
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        enabled,
                                        tr(job.key()),
                                    )
                                });
                                self.round_targets.push((RoundCommand::Job(job), rect));
                                if response.clicked() {
                                    self.round_command = Some(RoundCommand::Job(job));
                                }
                            }
                            if !crew.feedback.is_empty() {
                                ui.small(tr(&crew.feedback));
                            }
                        };
                        if wide {
                            ui.horizontal_top(|ui| {
                                preview(ui);
                                ui.add_space(12.);
                                ui.vertical(|ui| jobs(ui));
                            });
                        } else {
                            preview(ui);
                            ui.add_space(10.);
                            jobs(ui);
                        }
                    }
                    ui.add_space(20.);
                    ui.separator();
                    ui.add_space(10.);
                    let response = ui.add_sized(
                        [240., 46.],
                        egui::Button::new(
                            RichText::new(tr(if round.ready {
                                "lobby.unready"
                            } else {
                                "lobby.ready"
                            }))
                            .size(17.),
                        )
                        .fill(if round.ready {
                            SURFACE
                        } else {
                            Color32::from_rgb(65, 86, 79)
                        }),
                    );
                    self.round_targets
                        .push((RoundCommand::Ready(!round.ready), response.rect));
                    if response.clicked() {
                        self.round_command = Some(RoundCommand::Ready(!round.ready));
                    }
                    if round.ready {
                        ui.label(format!(
                            "{} {} {}",
                            tr("lobby.countdown"),
                            round.remaining.div_ceil(60),
                            tr("seconds")
                        ));
                    } else {
                        ui.small(tr("lobby.ready_hint"));
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draw(
        ctx: &egui::Context,
        ui: &mut Ui,
        f: &station_server::Frame,
        size: [f32; 2],
        events: Vec<egui::Event>,
    ) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |root| {
            ui.draw(root, f);
        });
        output.textures_delta.clear();
    }
    #[test]
    fn lobby_cards_fit_and_unavailable_job_cannot_be_clicked() {
        let session = station_server::Session::start_rounds(7);
        let mut f = session.frame();
        f.crew.as_mut().unwrap().vacancies[0] = 0;
        for size in [[480., 900.], [960., 720.]] {
            let ctx = egui::Context::default();
            let mut ui = Ui::default();
            for _ in 0..3 {
                draw(&ctx, &mut ui, &f, size, vec![]);
            }
            assert_eq!(ui.round_targets.len(), 4);
            for (_, rect) in &ui.round_targets {
                assert!(
                    rect.left() >= 0. && rect.right() <= size[0],
                    "card outside viewport: {rect:?}"
                );
            }
            let pos = ui.round_targets[0].1.center();
            let events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ];
            draw(&ctx, &mut ui, &f, size, events);
            assert!(ui.round_command.is_none(), "disabled job emitted a command");
        }
    }
    #[test]
    fn device_panels_are_opt_in_and_close_out_of_reach() {
        let session =
            station_server::Session::start_content(7, station_server::Content::test_station());
        let mut f = session.frame();
        f.round = None;
        let mut sim = station_sim::Simulation::new(7);
        sim.prepare_departments();
        f.doors = sim.door_snapshots();
        f.doors[0].reachable = true;
        let ctx = egui::Context::default();
        let mut ui = Ui::default();
        draw(&ctx, &mut ui, &f, [1100., 760.], vec![]);
        assert!(!ui.targets.iter().any(|(c, _)| matches!(
            c,
            station_server::Command::Door(_) | station_server::Command::PowerSwitch { .. }
        )));
        ui.device = Some(f.doors[0].cell);
        draw(&ctx, &mut ui, &f, [1100., 760.], vec![]);
        assert!(
            ui.targets
                .iter()
                .any(|(c, _)| *c == station_server::Command::Door(0))
        );
        f.doors[0].reachable = false;
        draw(&ctx, &mut ui, &f, [1100., 760.], vec![]);
        assert!(ui.device.is_none());
    }
    #[test]
    fn equipment_slots_remain_distinct_and_inside_window() {
        let session = station_server::Session::start(7);
        let f = session.frame();
        let ctx = egui::Context::default();
        let mut ui = Ui::default();
        ui.inventory = true;
        for _ in 0..4 {
            draw(&ctx, &mut ui, &f, [640., 720.], vec![]);
        }
        let slots: Vec<_> = ui
            .targets
            .iter()
            .filter(|(c, _)| matches!(c, station_server::Command::Slot(_)))
            .map(|(_, r)| *r)
            .collect();
        assert_eq!(slots.len(), 16);
        for (i, rect) in slots.iter().enumerate() {
            assert!(rect.left() >= 0. && rect.right() <= 640. && rect.bottom() <= 720.);
            assert!(
                slots
                    .iter()
                    .skip(i + 1)
                    .all(|other| !rect.intersects(*other))
            );
        }
    }
}
