//! Stable, visibility-filtered world object picker shared by all right-click targets.
use crate::{sprites::Icon, ui::tr};
use station_server::{Command, Frame};
use station_sim::entity::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Item(usize, EntityId),
    Door(usize, EntityId),
    Power(bool, usize, [i32; 3]),
}
impl Target {
    pub fn position(self, f: &Frame) -> Option<[f32; 3]> {
        let cell = match self {
            Self::Item(id, entity) => {
                return f
                    .items
                    .get(id)
                    .filter(|i| i.entity == Some(entity))
                    .map(|i| i.position);
            }
            Self::Door(id, entity) => {
                f.doors
                    .iter()
                    .find(|d| d.id == id && d.entity == entity)?
                    .cell
            }
            Self::Power(source, id, cell) => {
                let n = if source {
                    &f.power.sources
                } else {
                    &f.power.controllers
                }
                .get(id)?;
                if n.cell != cell {
                    return None;
                }
                cell
            }
        };
        Some([
            cell[0] as f32 + 0.5,
            cell[1] as f32 + 0.5,
            cell[2] as f32 + 1.,
        ])
    }
    fn visible(self, f: &Frame, visible: &impl Fn([f32; 3]) -> bool) -> bool {
        self.position(f).is_some_and(|p| {
            (if p[2] >= 0.5 { 0 } else { -4 }) == (if f.player.position[2] >= 0.5 { 0 } else { -4 })
                && (visible(p)
                    || matches!(self, Self::Door(..))
                        && [[-0.51, 0.], [0.51, 0.], [0., -0.51], [0., 0.51]]
                            .iter()
                            .any(|d| visible([p[0] + d[0], p[1] + d[1], p[2]])))
        }) && match self {
            Self::Item(id, _) => f.items[id].visible && !f.items[id].held,
            _ => true,
        }
    }
    fn name(self, f: &Frame) -> String {
        match self {
            Self::Item(id, _) => tr(&f.items[id].name).into(),
            Self::Door(id, _) => f
                .doors
                .iter()
                .find(|d| d.id == id)
                .map(|d| format!("{} · {}", tr("object.airlock"), tr(&d.name)))
                .unwrap_or_default(),
            Self::Power(source, id, _) => format!(
                "{} {}",
                tr(if source {
                    "power.source"
                } else {
                    "power.controller"
                }),
                id + 1
            ),
        }
    }
    fn description(self, f: &Frame) -> String {
        match self {
            Self::Item(id, _) => tr(&f.items[id].description).into(),
            Self::Door(id, _) => {
                let d = f.doors.iter().find(|d| d.id == id).unwrap();
                format!(
                    "{}
{} · {}",
                    tr("object.door_description"),
                    tr(if d.open {
                        "object.open"
                    } else {
                        "object.closed"
                    }),
                    tr(if d.powered { "power.on" } else { "power.off" })
                )
            }
            Self::Power(source, id, _) => {
                let n = &if source {
                    &f.power.sources
                } else {
                    &f.power.controllers
                }[id];
                format!(
                    "{}
{}",
                    tr(if source {
                        "object.source_description"
                    } else {
                        "object.controller_description"
                    }),
                    tr(if n.powered { "power.on" } else { "power.off" })
                )
            }
        }
    }
}
pub fn pick(
    f: &Frame,
    world: [f32; 2],
    deck: i32,
    visible: impl Fn([f32; 3]) -> bool,
    hit: impl Fn(&station_sim::ItemSnapshot) -> bool,
) -> Vec<Target> {
    let mut items: Vec<_> = f
        .items
        .iter()
        .filter(|i| {
            i.entity.is_some()
                && i.visible
                && !i.held
                && visible(i.position)
                && i.position[2] >= deck as f32
                && i.position[2] - i.half <= deck as f32 + 3.
                && if i.layers.is_empty() {
                    (i.position[0] - world[0]).abs() <= i.half + 0.15
                        && (i.position[1] - world[1]).abs() <= i.half + 0.15
                } else {
                    hit(i)
                }
        })
        .collect();
    items.sort_by(|a, b| {
        b.sprite_style
            .draw_depth
            .cmp(&a.sprite_style.draw_depth)
            .then_with(|| a.position[1].total_cmp(&b.position[1]))
            .then_with(|| b.id.cmp(&a.id))
    });
    let mut result: Vec<_> = items
        .into_iter()
        .map(|i| Target::Item(i.id, i.entity.unwrap()))
        .collect();
    let cell = [world[0].floor() as i32, world[1].floor() as i32, deck];
    for d in &f.doors {
        let target = Target::Door(d.id, d.entity);
        if d.cell == [cell[0], cell[1], deck + 1] && target.visible(f, &visible) {
            result.push(target);
        }
    }
    for (source, nodes) in [(true, &f.power.sources), (false, &f.power.controllers)] {
        for (id, n) in nodes.iter().enumerate() {
            let target = Target::Power(source, id, n.cell);
            if n.cell == cell && target.visible(f, &visible) {
                result.push(target);
            }
        }
    }
    result
}
#[derive(Default)]
pub struct Objects {
    pub entries: Vec<Target>,
    pub row_targets: Vec<(Target, egui::Rect)>,
    pub icons: Vec<(Target, Icon)>,
    pub camera: [f32; 3],
    pos: egui::Pos2,
    selected: Option<Target>,
    pub inspection: Option<Target>,
    pub hovered: Option<Target>,
    pub examine_rect: Option<egui::Rect>,
}
impl Objects {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.selected = None;
        self.inspection = None;
        self.hovered = None;
    }
    pub fn open(&mut self, entries: Vec<Target>, pos: egui::Pos2) {
        self.entries = entries;
        self.selected = None;
        self.inspection = None;
        self.pos = pos;
    }
    pub fn retain_visible(&mut self, f: &Frame, visible: impl Fn([f32; 3]) -> bool) {
        self.entries.retain(|t| t.visible(f, &visible));
        if self.selected.is_some_and(|t| !t.visible(f, &visible)) {
            self.selected = None;
        }
        if self.inspection.is_some_and(|t| t.position(f).is_none() || (!t.visible(f,&visible) && !matches!(t, Target::Item(id,_) if f.inventory.hands.contains(&Some(id)) || f.inventory.slots.contains(&Some(id))))) { self.inspection=None; }
    }
    pub fn examine_item(&mut self, f: &Frame, id: usize) {
        self.inspection = f
            .items
            .get(id)
            .and_then(|i| i.entity.map(|e| Target::Item(id, e)));
    }
    fn icon(&self, target: Target, items: &[Option<Icon>]) -> Option<Icon> {
        match target {
            Target::Item(id, _) => items.get(id).copied().flatten(),
            _ => self
                .icons
                .iter()
                .find(|(t, _)| *t == target)
                .map(|(_, i)| *i),
        }
    }
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        f: &Frame,
        icons: &[Option<Icon>],
        commands: &mut Vec<Command>,
    ) -> Option<(usize, egui::Pos2)> {
        self.examine_rect = None;
        self.row_targets.clear();
        self.hovered = None;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.clear();
        }
        let mut chosen = None;
        if !self.entries.is_empty() {
            let result = egui::Window::new(tr("object.targets"))
                .id(egui::Id::unique("object-picker"))
                .fixed_pos(self.pos)
                .resizable(false)
                .collapsible(false)
                .order(egui::Order::Foreground)
                .default_width(220.)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .show(ui, |ui| {
                            for &target in &self.entries {
                                let name = target.name(f);
                                let button = if let Some((texture, uv, _)) =
                                    self.icon(target, icons)
                                {
                                    egui::Button::image_and_text(
                                        egui::Image::new((texture, egui::vec2(28., 28.))).uv(uv),
                                        name,
                                    )
                                } else {
                                    egui::Button::new(name)
                                };
                                let r = ui.add_sized([210., 32.], button.frame(false));
                                self.row_targets.push((target, r.rect));
                                if r.hovered() {
                                    self.hovered = Some(target);
                                }
                                if r.clicked() {
                                    chosen = Some(target);
                                }
                            }
                        });
                });
            if result.is_some_and(|r| r.response.clicked_elsewhere())
                && ctx.input(|i| i.pointer.primary_clicked())
            {
                self.entries.clear();
            }
        }
        let mut item_menu = None;
        if let Some(target) = chosen {
            self.entries.clear();
            match target {
                Target::Item(id, _) => item_menu = Some((id, self.pos)),
                _ => self.selected = Some(target),
            }
        }
        if let Some(target) = self.selected
            && target.position(f).is_some()
        {
            let mut open = true;
            egui::Window::new(target.name(f))
                .id(egui::Id::unique("object-actions"))
                .default_width(220.)
                .default_pos(self.pos)
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    crate::ui_visuals::item_image(ui, self.icon(target, icons), 32.);
                    let mut action = |command, enabled, label: &str| {
                        let r = ui.add_enabled(enabled, egui::Button::new(tr(label)));
                        if r.clicked() {
                            commands.push(command);
                        }
                    };
                    match target {
                        Target::Door(id, _) => {
                            let d = f.doors.iter().find(|d| d.id == id).unwrap();
                            action(
                                Command::Door(id),
                                d.reachable && d.allowed && d.powered && !d.phase.moving(),
                                if d.open { "door.close" } else { "door.open" },
                            );
                            if !d.powered
                                && let Some(tool) = f.inventory.hands[f.inventory.active]
                                && f.items[tool].kind == station_sim::ItemKind::Crowbar
                            {
                                action(
                                    Command::Work {
                                        target: station_server::WorkTarget::Door(id),
                                        tool,
                                    },
                                    d.reachable
                                        && !d.open
                                        && !d.phase.moving()
                                        && f.action.is_none(),
                                    "power.pry",
                                );
                            }
                        }
                        Target::Power(source, id, _) => {
                            let n = &if source {
                                &f.power.sources
                            } else {
                                &f.power.controllers
                            }[id];
                            action(
                                Command::PowerSwitch { source, id },
                                n.reachable,
                                if n.enabled {
                                    "power.disable"
                                } else {
                                    "power.enable"
                                },
                            );
                        }
                        _ => {}
                    }
                    let examine = ui.button(tr("verb.examine"));
                    self.examine_rect = Some(examine.rect);
                    if examine.clicked() {
                        self.inspection = Some(target);
                    }
                });
            if !open {
                self.selected = None;
            }
        }
        if let Some(target) = self.inspection
            && target.position(f).is_some()
        {
            let mut open = true;
            egui::Window::new(tr("verb.examine"))
                .id(egui::Id::unique("object-examine"))
                .open(&mut open)
                .default_width(280.)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        crate::ui_visuals::item_image(ui, self.icon(target, icons), 64.);
                        ui.strong(target.name(f));
                    });
                    ui.separator();
                    ui.label(target.description(f));
                });
            if !open {
                self.inspection = None;
            }
        }
        if let Some(target) = self.hovered
            && let Some(p) = target.position(f)
        {
            let screen = ctx.content_rect();
            let scale = screen.height() / 18.;
            let center = screen.center()
                + egui::vec2(
                    (p[0] - self.camera[0]) * scale,
                    (self.camera[1] - p[1]) * scale,
                );
            let size = match target {
                Target::Item(id, _) => f.items[id].sprite_size,
                _ => [1., 1.],
            };
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::unique("world-selection"),
            ))
            .rect_stroke(
                egui::Rect::from_center_size(
                    center,
                    egui::vec2(size[0].max(0.4) * scale, size[1].max(0.4) * scale),
                ),
                0.,
                egui::Stroke::new(2., egui::Color32::LIGHT_GREEN),
                egui::StrokeKind::Outside,
            );
        }
        item_menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Frame {
        station_server::Session::start(7).frame()
    }
    #[test]
    fn stacked_items_devices_and_decks_are_filtered_without_losing_targets() {
        let mut f = frame();
        for i in &mut f.items {
            i.visible = false;
        }
        for id in [0, 2] {
            f.items[id].visible = true;
            f.items[id].position = [0.5, 0.5, 1.2];
            f.items[id].layers.clear();
        }
        let mut sim = station_sim::Simulation::new(7);
        sim.prepare_departments();
        f.doors = sim.door_snapshots();
        f.doors[0].cell = [0, 0, 1];
        f.power.controllers = vec![station_sim::power::NodeSnapshot {
            cell: [0, 0, 0],
            enabled: true,
            powered: true,
            reachable: true,
        }];
        let targets = pick(&f, [0.5, 0.5], 0, |_| true, |_| true);
        assert_eq!(targets.len(), 4);
        assert!(targets.iter().any(|t| matches!(t, Target::Door(..))));
        assert!(targets.iter().any(|t| matches!(t, Target::Power(..))));
        assert!(pick(&f, [0.5, 0.5], 0, |_| false, |_| true).is_empty());
        assert!(pick(&f, [0.5, 0.5], -4, |_| true, |_| true).is_empty());
        let mut objects = Objects::default();
        objects.open(targets, egui::Pos2::ZERO);
        f.items[0].entity = None;
        objects.retain_visible(&f, |_| true);
        assert_eq!(objects.entries.len(), 3);
    }
    #[test]
    fn selector_hover_and_click_resolve_the_exact_row() {
        let f = frame();
        let ctx = egui::Context::default();
        let mut objects = Objects::default();
        let first = Target::Item(0, f.items[0].entity.unwrap());
        let second = Target::Item(2, f.items[2].entity.unwrap());
        objects.open(vec![first, second], egui::pos2(30., 30.));
        let run = |objects: &mut Objects, events| {
            let mut chosen = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |_| {
                    chosen = objects.draw(&ctx, &f, &[], &mut vec![]);
                },
            );
            output.textures_delta.clear();
            chosen
        };
        for _ in 0..3 {
            run(&mut objects, vec![]);
        }
        let pos = objects.row_targets[1].1.center();
        run(&mut objects, vec![egui::Event::PointerMoved(pos)]);
        assert_eq!(objects.hovered, Some(second));
        let mut events = vec![];
        for pressed in [true, false] {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        assert_eq!(run(&mut objects, events).map(|(id, _)| id), Some(2));
        objects.examine_item(&f, 2);
        assert_eq!(objects.inspection, Some(second));
        let mut changed = f.clone();
        changed.items[2].entity = None;
        objects.retain_visible(&changed, |_| true);
        assert!(objects.inspection.is_none());
    }
}
