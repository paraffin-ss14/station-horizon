//! Real pointer-input smoke for the lobby, inventory and right-click menu.
use station_server::{Frame, Job, Phase, RoundCommand};
pub struct Smoke {
    phase: u8,
    start: std::time::Instant,
    frames: u32,
    last: std::time::Instant,
}
impl Default for Smoke {
    fn default() -> Self {
        Self {
            phase: 0,
            start: std::time::Instant::now(),
            frames: 0,
            last: std::time::Instant::now(),
        }
    }
}
fn click(raw: &mut egui::RawInput, rect: egui::Rect, button: egui::PointerButton) {
    let pos = rect.center();
    raw.events.push(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        raw.events.push(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        });
    }
}
impl Smoke {
    pub fn step(
        &mut self,
        ui: &mut crate::ui::Ui,
        f: &Frame,
        raw: &mut egui::RawInput,
        renderer: &mut horizon_station_render::Renderer,
    ) -> bool {
        assert!(
            self.start.elapsed().as_secs() < 40,
            "shell UI smoke timed out at {}",
            self.phase
        );
        self.frames += 1;
        if self.frames < 12 || self.last.elapsed().as_millis() < 400 {
            return false;
        }
        let Some(round) = f.round else {
            return false;
        };
        let mut next = false;
        match self.phase {
            0 if round.phase == Phase::Lobby => {
                if let Some((_, rect)) = ui
                    .round_targets
                    .iter()
                    .find(|(c, _)| matches!(c, RoundCommand::Job(Job::Engineer)))
                {
                    click(raw, *rect, egui::PointerButton::Primary);
                    next = true;
                }
            }
            1 if f.crew.as_ref().is_some_and(|c| c.selected == Job::Engineer) => {
                std::fs::create_dir_all("station/evidence").unwrap();
                renderer.capture_next("station/evidence/lobby.png");
                next = true;
            }
            2 => {
                if let Some((_, rect)) = ui
                    .round_targets
                    .iter()
                    .find(|(c, _)| matches!(c, RoundCommand::Ready(true)))
                {
                    click(raw, *rect, egui::PointerButton::Primary);
                    next = true;
                }
            }
            3 if round.phase == Phase::Playing && f.player.tick >= 30 => {
                assert_eq!(f.crew.as_ref().unwrap().selected, Job::Engineer);
                ui.inventory = true;
                next = true;
            }
            4 => {
                renderer.capture_next("station/evidence/inventory.png");
                next = true;
            }
            5 => {
                let rect = ui
                    .targets
                    .iter()
                    .find(|(c, _)| *c == station_server::Command::Slot(11))
                    .expect("pocket UI target")
                    .1;
                assert!(f.inventory.slots[11].is_some());
                click(raw, rect, egui::PointerButton::Secondary);
                next = true;
            }
            6 if ui.menu.is_some() => {
                renderer.capture_next("station/evidence/context.png");
                next = true;
            }
            7 => {
                let target = f.inventory.slots[11].unwrap();
                let rect = ui
                    .targets
                    .iter()
                    .find(|(c, _)| {
                        *c == station_server::Command::Verb {
                            target,
                            kind: station_server::VerbKind::Examine,
                        }
                    })
                    .expect("examine UI target")
                    .1;
                click(raw, rect, egui::PointerButton::Primary);
                next = true;
            }
            8..=17 => {
                assert!(ui.menu.is_none(), "context menu must be closed");
                if self.phase == 8 {
                    assert!(ui.objects.inspection.is_some());
                    renderer.capture_next("station/evidence/examine-item.png");
                }
                if self.phase == 9 {
                    ui.objects.inspection = None;
                }
                let phase = self.phase - 8;
                let (from, to) = if phase < 5 {
                    (
                        station_server::Command::Slot(11),
                        station_server::Command::Hand(0),
                    )
                } else {
                    (
                        station_server::Command::Hand(0),
                        station_server::Command::Slot(11),
                    )
                };
                let rect = |command| {
                    ui.targets
                        .iter()
                        .find(|(c, _)| *c == command)
                        .expect("drag target")
                        .1
                };
                let pos = if phase % 5 < 2 {
                    rect(from).center() + egui::vec2((phase % 5) as f32 * 12., 0.)
                } else {
                    rect(to).center()
                };
                raw.events.push(egui::Event::PointerMoved(pos));
                match phase % 5 {
                    0 | 3 => raw.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: phase.is_multiple_of(5),
                        modifiers: Default::default(),
                    }),
                    2 => renderer.capture_next("station/evidence/drag.png"),
                    4 => {
                        if phase < 5 {
                            assert!(f.inventory.hands[0].is_some());
                            assert!(f.inventory.slots[11].is_none());
                        } else {
                            assert!(f.inventory.hands[0].is_none());
                            assert!(f.inventory.slots[11].is_some());
                        }
                    }
                    _ => {}
                }
                next = true;
            }
            18 => {
                let d = &f.doors[0];
                let world = [d.cell[0] as f32 + 0.5, d.cell[1] as f32 + 0.5];
                let targets = crate::object_menu::pick(
                    f,
                    world,
                    d.cell[2] - 1,
                    |p| renderer.is_visible(p),
                    |_| false,
                );
                assert!(
                    targets
                        .iter()
                        .any(|t| matches!(t, crate::object_menu::Target::Door(..))),
                    "visible closed door must be selectable"
                );
                ui.objects.open(targets, egui::pos2(680., 190.));
                next = true;
            }
            19 => {
                let pos = ui.objects.row_targets[0].1.center();
                raw.events.push(egui::Event::PointerMoved(pos));
                renderer.capture_next("station/evidence/object-picker.png");
                next = true;
            }
            20 => {
                assert!(ui.objects.hovered.is_some());
                click(
                    raw,
                    ui.objects.row_targets[0].1,
                    egui::PointerButton::Primary,
                );
                next = true;
            }
            21 => {
                click(
                    raw,
                    ui.objects.examine_rect.expect("device examine button"),
                    egui::PointerButton::Primary,
                );
                next = true;
            }
            22 => {
                assert!(ui.objects.inspection.is_some());
                renderer.capture_next("station/evidence/examine-device.png");
                next = true;
            }
            23 => {
                println!(
                    "Shell UI passed: lobby, context, drag, visible door picker, hover highlight and item/device examination."
                );
                return true;
            }
            _ => {}
        }
        if next {
            self.phase += 1;
            self.frames = 0;
            self.last = std::time::Instant::now();
        }
        false
    }
}
