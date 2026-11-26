use station_server::{Command, Frame, SLOTS};
#[derive(Default)]
pub struct Ui {
    pub credits: bool,
    menu_identity: Option<(usize, Option<station_sim::entity::EntityId>)>,
    pub objects: crate::object_menu::Objects,
    pub device: Option<[i32; 3]>,
    drag: Option<(usize, usize)>,
    drag_epoch: u64,
    pub portraits: Vec<egui::TextureId>,
    pub round_targets: Vec<(station_server::RoundCommand, egui::Rect)>,
    pub sprite_message: String,
    slot_texture: Option<egui::TextureHandle>,
    pub sprite_icons: Vec<Option<crate::sprites::Icon>>,
    pub round_command: Option<station_server::RoundCommand>,
    pub targets: Vec<(Command, egui::Rect)>,
    pub inventory: bool,
    pub floor: Option<[i32; 3]>,
    pub menu: Option<(usize, egui::Pos2)>,
}
pub fn floor_target(cell: [i32; 3], kind: station_sim::ItemKind) -> station_server::WorkTarget {
    if kind == station_sim::ItemKind::Wirecutters {
        station_server::WorkTarget::Cable(cell)
    } else if matches!(kind, station_sim::ItemKind::Tile(_)) {
        station_server::WorkTarget::LayTile(cell)
    } else {
        station_server::WorkTarget::LiftTile(cell)
    }
}
pub fn tr(key: &str) -> &str {
    include_str!("ru.txt")
        .lines()
        .find_map(|l| l.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
        .unwrap_or(key)
}
impl Ui {
    fn draw_slot(
        &mut self,
        ui: &mut egui::Ui,
        f: &Frame,
        slot: usize,
        icons: &[Option<crate::sprites::Icon>],
        texture: egui::TextureId,
        commands: &mut Vec<Command>,
    ) {
        let item = f.inventory.slots[slot];
        let response = crate::ui_visuals::equipment_slot(
            ui,
            tr(SLOTS[slot].name),
            item.and_then(|id| icons.get(id).copied().flatten()),
            slot,
            texture,
        )
        .on_hover_text(
            item.and_then(|id| f.items.get(id))
                .map_or(tr("empty"), |i| tr(&i.name)),
        );
        if response.drag_started()
            && let Some(item) = item
        {
            self.drag = Some((item, slot));
        }
        self.targets.push((Command::Slot(slot), response.rect));
        if response.clicked() {
            commands.push(Command::Slot(slot));
        }
        if response.secondary_clicked()
            && let Some(id) = item
        {
            self.menu = Some((id, response.rect.right_top()));
        }
    }
    pub fn draw(&mut self, root: &mut egui::Ui, f: &Frame) -> Vec<Command> {
        crate::ui_visuals::theme(root.ctx());
        crate::sprites::credits(root.ctx(), &mut self.credits);
        egui::Panel::top("sprite-credits").show(root, |ui| {
            if ui.small_button(tr("sprites.credits")).clicked() {
                self.credits = true;
            }
            if !self.sprite_message.is_empty() {
                ui.label(&self.sprite_message);
            }
        });
        self.targets.clear();
        self.round_targets.clear();
        self.round_command = None;
        if f.local_id != 0 {
            egui::Panel::top("network").show(root, |ui| {
                ui.label(format!(
                    "{}: {:.0} ms · {}: {}",
                    tr("network.ping"),
                    f.rtt_ms,
                    tr("network.players"),
                    f.peers.len()
                ));
                if f.received_at.is_some_and(|t| t.elapsed().as_millis() > 250) {
                    ui.label(tr("network.waiting"));
                }
            });
        }
        let epoch = f.round.map_or(0, |r| r.epoch);
        if self.drag_epoch != epoch || !f.connected {
            self.drag = None;
            self.objects.clear();
            self.device = None;
            self.drag_epoch = epoch;
        }
        if !f.connected {
            root.label(tr("network.disconnected"));
            return vec![];
        }
        if let Some(r) = f.round {
            use station_server::{Phase, RoundCommand};
            if r.phase == Phase::Lobby {
                self.lobby(root, f);
            } else {
                egui::Panel::top("round").show(root, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("{} {:03}", tr("round.number"), r.number));
                        ui.separator();
                        if r.phase == Phase::Playing {
                            ui.label(format!(
                                "{} {:02}:{:02}",
                                tr("round.remaining"),
                                r.remaining / 3600,
                                r.remaining / 60 % 60
                            ));
                            if let Some(crew) = &f.crew {
                                ui.label(tr(crew.selected.key()));
                            }
                            if f.host && ui.button(tr("round.end")).clicked() {
                                self.round_command = Some(RoundCommand::End);
                            }
                        } else {
                            ui.label(format!(
                                "{}: {} {} · {}: {}",
                                tr("round.results"),
                                r.elapsed / 60,
                                tr("seconds"),
                                tr("round.changes"),
                                r.world_changes
                            ));
                            if f.host && ui.button(tr("round.lobby")).clicked() {
                                self.round_command = Some(RoundCommand::Lobby);
                            }
                        }
                    });
                });
            }
            if r.phase != Phase::Playing {
                self.objects.clear();
                self.device = None;
                self.drag = None;
                self.inventory = false;
                self.floor = None;
                self.menu = None;
                return vec![];
            }
        }
        let ctx = root.ctx().clone();
        let mut commands = vec![];
        if let Some((item, pos)) = self
            .objects
            .draw(&ctx, f, &self.sprite_icons, &mut commands)
        {
            self.menu = Some((item, pos));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.device = None;
            self.drag = None;
        }
        if self.device.is_some_and(|cell| {
            !f.doors.iter().any(|d| d.cell == cell && d.reachable)
                && !f
                    .power
                    .sources
                    .iter()
                    .chain(&f.power.controllers)
                    .any(|n| n.cell == cell && n.reachable)
        }) {
            self.device = None;
        }
        let mut device_open = true;
        if f.doors
            .iter()
            .any(|d| d.reachable && Some(d.cell) == self.device)
        {
            egui::Window::new(tr("door.title"))
                .id(egui::Id::unique("doors"))
                .open(&mut device_open)
                .default_pos([700., 70.])
                .show(&ctx, |ui| {
                    for door in f
                        .doors
                        .iter()
                        .filter(|d| d.reachable && Some(d.cell) == self.device)
                    {
                        ui.label(tr(&door.name));
                        let response = ui.add_enabled(
                            door.allowed && door.powered && !door.phase.moving(),
                            egui::Button::new(tr(if door.phase.moving() {
                                "door.moving"
                            } else if door.open {
                                "door.close"
                            } else {
                                "door.open"
                            })),
                        );
                        self.targets.push((Command::Door(door.id), response.rect));
                        if response.clicked() {
                            commands.push(Command::Door(door.id));
                        }
                        if !door.powered {
                            ui.label(tr("power.no_power"));
                            if let Some(tool) = f.inventory.hands[f.inventory.active]
                                && f.items[tool].kind == station_sim::ItemKind::Crowbar
                            {
                                let command = Command::Work {
                                    target: station_server::WorkTarget::Door(door.id),
                                    tool,
                                };
                                let r = ui.add_enabled(
                                    !door.open && !door.phase.moving() && f.action.is_none(),
                                    egui::Button::new(tr("power.pry")),
                                );
                                self.targets.push((command, r.rect));
                                if r.clicked() {
                                    commands.push(command);
                                }
                            }
                        }
                        if !door.allowed {
                            ui.label(tr("error.access"));
                        }
                    }
                });
        }
        if f.power
            .sources
            .iter()
            .chain(&f.power.controllers)
            .any(|n| n.reachable && Some(n.cell) == self.device)
        {
            egui::Window::new(tr("power.title"))
                .open(&mut device_open)
                .default_pos([700., 240.])
                .show(&ctx, |ui| {
                    for (source, nodes) in [(true, &f.power.sources), (false, &f.power.controllers)]
                    {
                        for (id, n) in nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| n.reachable && Some(n.cell) == self.device)
                        {
                            ui.label(format!(
                                "{} {}: {}",
                                tr(if source {
                                    "power.source"
                                } else {
                                    "power.controller"
                                }),
                                id + 1,
                                tr(if n.powered { "power.on" } else { "power.off" })
                            ));
                            let command = Command::PowerSwitch { source, id };
                            let r = ui.button(tr(if n.enabled {
                                "power.disable"
                            } else {
                                "power.enable"
                            }));
                            self.targets.push((command, r.rect));
                            if r.clicked() {
                                commands.push(command);
                            }
                        }
                    }
                });
        }
        if !device_open {
            self.device = None;
        }
        let icons = self.sprite_icons.clone();
        let slot_texture = self
            .slot_texture
            .get_or_insert_with(|| crate::sprites::slot_texture(&ctx))
            .id();
        let name = |id: Option<usize>| {
            id.and_then(|id| f.items.get(id)).map_or_else(
                || tr("empty").to_string(),
                |i| {
                    if matches!(i.kind, station_sim::ItemKind::Tile(_)) {
                        format!("{} ×{}", tr(&i.name), i.stack_count)
                    } else {
                        tr(&i.name).to_string()
                    }
                },
            )
        };
        egui::Panel::bottom("hands")
            .frame(egui::Frame::NONE.inner_margin(6))
            .show(root, |ui| {
                if !f.inventory.message.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(tr(&f.inventory.message));
                    });
                }

                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 600.) / 2.).max(0.));
                    if ui
                        .add_sized([64., 64.], egui::Button::new("I").selected(self.inventory))
                        .on_hover_text(tr("inventory"))
                        .clicked()
                    {
                        self.inventory = !self.inventory;
                    }
                    for slot in [5, 8, 10] {
                        self.draw_slot(ui, f, slot, &icons, slot_texture, &mut commands);
                    }
                    for hand in 0..2 {
                        let response = crate::ui_visuals::equipment_slot(
                            ui,
                            &format!(
                                "{}: {}",
                                tr(if hand == 0 { "left" } else { "right" }),
                                name(f.inventory.hands[hand])
                            ),
                            f.inventory.hands[hand].and_then(|id| icons.get(id).copied().flatten()),
                            16 + hand,
                            slot_texture,
                        );
                        if f.inventory.active == hand {
                            ui.painter().rect_stroke(
                                response.rect.shrink(1.),
                                0.,
                                egui::Stroke::new(2., egui::Color32::from_rgb(190, 210, 235)),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if response.drag_started()
                            && let Some(item) = f.inventory.hands[hand]
                        {
                            self.drag = Some((item, 16 + hand));
                        }
                        self.targets.push((Command::Hand(hand), response.rect));
                        if response.clicked() {
                            commands.push(Command::Hand(hand));
                        }
                        if response.secondary_clicked()
                            && let Some(id) = f.inventory.hands[hand]
                        {
                            self.menu = Some((id, response.rect.left_top()));
                        }
                    }
                    for slot in [11, 12, 15] {
                        self.draw_slot(ui, f, slot, &icons, slot_texture, &mut commands);
                    }
                });
            });
        if let Some(a) = f.action {
            egui::Window::new(tr("action.title"))
                .id(egui::Id::unique("action"))
                .fixed_pos([360., 430.])
                .resizable(false)
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label(tr(match a.target {
                        station_server::WorkTarget::Cable(_) => "power.cable_work",
                        station_server::WorkTarget::Door(_) => "power.pry",
                        station_server::WorkTarget::Lock => "action.lock",
                        station_server::WorkTarget::Floor(_) => "action.floor",
                        station_server::WorkTarget::LiftTile(_) => "action.lift_tile",
                        station_server::WorkTarget::LayTile(_) => "action.lay_tile",
                    }));
                    ui.add(
                        egui::ProgressBar::new(a.elapsed as f32 / a.duration as f32)
                            .desired_width(260.)
                            .show_percentage(),
                    );
                    ui.small(tr("action.hint"));
                    let command = Command::CancelWork(a.id);
                    let r = ui.button(tr("action.cancel"));
                    self.targets.push((command, r.rect));
                    if r.clicked() {
                        commands.push(command);
                    }
                });
        }
        if let Some(cell) = self.floor {
            egui::Window::new(tr("floor.title"))
                .id(egui::Id::unique("floor"))
                .default_pos([18., 40.])
                .show(&ctx, |ui| {
                    ui.label(format!("{}, {}, {}", cell[0], cell[1], cell[2]));
                    if let Some(tool) = f.inventory.hands[f.inventory.active] {
                        let kind = f.items[tool].kind;
                        let target = floor_target(cell, kind);
                        let command = Command::Work { target, tool };
                        let label = if matches!(target, station_server::WorkTarget::Cable(_)) {
                            "power.cable_work"
                        } else if matches!(target, station_server::WorkTarget::LayTile(_)) {
                            "action.lay_tile"
                        } else {
                            "action.lift_tile"
                        };
                        let r = ui.add_enabled(
                            f.action.is_none() && f.cells.contains(&cell),
                            egui::Button::new(tr(label)),
                        );
                        self.targets.push((command, r.rect));
                        if r.clicked() {
                            commands.push(command);
                        }
                        if kind == station_sim::ItemKind::Crowbar {
                            ui.separator();
                            let command = Command::Work {
                                target: station_server::WorkTarget::Floor(cell),
                                tool,
                            };
                            let r = ui.add_enabled(
                                f.action.is_none() && f.cells.contains(&cell),
                                egui::Button::new(tr("action.floor")),
                            );
                            self.targets.push((command, r.rect));
                            if r.clicked() {
                                commands.push(command);
                            }
                        }
                    } else {
                        ui.label(tr("action.need_tile_tool"));
                    }
                    if ui.button(tr("close")).clicked() {
                        self.floor = None;
                    }
                });
        }
        if self.inventory {
            egui::Window::new(tr("inventory"))
                .id(egui::Id::unique("equipment-hud"))
                .anchor(egui::Align2::CENTER_BOTTOM, [-200., -82.])
                .auto_sized()
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .frame(
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(34, 35, 40))
                        .inner_margin(3)
                        .stroke(egui::Stroke::new(1., egui::Color32::from_gray(100))),
                )
                .show(&ctx, |ui| {
                    egui::Grid::new("slots")
                        .num_columns(3)
                        .spacing([3., 3.])
                        .show(ui, |ui| {
                            // SS14 human default group, top-to-bottom because its hotbar expands backwards.
                            // Legs is our additional slot; pockets, belt, ID, back and suit storage stay on the HUD.
                            for row in [
                                [Some(1), Some(0), None],
                                [Some(4), Some(3), Some(2)],
                                [Some(7), Some(6), Some(9)],
                                [Some(13), Some(14), None],
                            ] {
                                for slot in row {
                                    if let Some(slot) = slot {
                                        self.draw_slot(
                                            ui,
                                            f,
                                            slot,
                                            &icons,
                                            slot_texture,
                                            &mut commands,
                                        );
                                    } else {
                                        ui.allocate_space(egui::vec2(64., 64.));
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        }
        if f.storage.open && f.storage.accessible {
            egui::Window::new(tr("storage.title"))
                .id(egui::Id::unique("storage"))
                .default_pos([650., 40.])
                .default_width(280.)
                .show(&ctx, |ui| {
                    ui.label(format!(
                        "{}: {} / {}",
                        tr("storage.capacity"),
                        f.storage.items.len(),
                        f.storage.capacity
                    ));
                    for &item in &f.storage.items {
                        let command = Command::Take {
                            container: f.storage.entity,
                            item,
                        };
                        let response = ui.add_enabled(
                            f.inventory.hands[f.inventory.active].is_none(),
                            egui::Button::new(format!(
                                "{} — {}",
                                name(Some(item)),
                                tr("storage.take")
                            )),
                        );
                        self.targets.push((command, response.rect));
                        if response.clicked() {
                            commands.push(command);
                        }
                    }
                    if f.storage.items.is_empty() {
                        ui.label(tr("storage.empty"));
                    }
                    if let Some(item) = f.inventory.hands[f.inventory.active] {
                        let command = Command::Store {
                            container: f.storage.entity,
                            item,
                        };
                        let response = ui.add_enabled(
                            f.storage.items.len() < f.storage.capacity && item != f.storage.entity,
                            egui::Button::new(tr("storage.store")),
                        );
                        self.targets.push((command, response.rect));
                        if response.clicked() {
                            commands.push(command);
                        }
                    }
                    let command = Command::Verb {
                        target: f.storage.entity,
                        kind: station_server::VerbKind::Close,
                    };
                    let response = ui.button(tr("verb.close_storage"));
                    self.targets.push((command, response.rect));
                    if response.clicked() {
                        commands.push(command);
                    }
                });
        }
        if let Some((id, _)) = self.menu {
            let entity = f.items.get(id).and_then(|i| i.entity);
            if entity.is_none()
                || self
                    .menu_identity
                    .is_some_and(|(old, e)| old == id && e != entity)
            {
                self.menu = None;
                self.menu_identity = None;
            } else {
                self.menu_identity = Some((id, entity));
            }
        } else {
            self.menu_identity = None;
        }
        if let Some((id, pos)) = self.menu {
            let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let popup = egui::Window::new(name(Some(id)))
                .id(egui::Id::unique("context"))
                .fixed_pos(pos)
                .resizable(false)
                .collapsible(false)
                .default_width(180.)
                .frame(
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(38, 39, 44))
                        .inner_margin(2)
                        .stroke(egui::Stroke::new(1., egui::Color32::from_gray(105))),
                )
                .title_bar(false)
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        crate::ui_visuals::item_image(ui, icons.get(id).copied().flatten(), 24.);
                        ui.label(name(Some(id)));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(360.)
                        .show(ui, |ui| {
                            if let Some(verbs) = f.verbs.get(id) {
                                for category in [
                                    "category.interaction",
                                    "category.equipment",
                                    "category.other",
                                ] {
                                    if !crate::ui_visuals::show_category(
                                        verbs.iter().filter(|v| v.category == category).count(),
                                    ) {
                                        continue;
                                    }
                                    let mut buttons = |ui: &mut egui::Ui| {
                                        for v in verbs.iter().filter(|v| v.category == category) {
                                            let response = ui
                                                .add_enabled(
                                                    v.enabled,
                                                    egui::Button::new(tr(v.label))
                                                        .min_size(egui::vec2(176., 32.))
                                                        .frame(false),
                                                )
                                                .on_disabled_hover_text(
                                                    crate::ui_visuals::tooltip(v.reason, v.label),
                                                );
                                            self.targets.push((
                                                Command::Verb {
                                                    target: id,
                                                    kind: v.kind,
                                                },
                                                response.rect,
                                            ));
                                            if response.clicked() {
                                                if v.kind == station_server::VerbKind::Examine {
                                                    self.objects.examine_item(f, id);
                                                }
                                                commands.push(Command::Verb {
                                                    target: id,
                                                    kind: v.kind,
                                                });
                                                close = true;
                                            }
                                        }
                                    };
                                    if category == "category.equipment" {
                                        ui.collapsing(tr(category), buttons);
                                    } else {
                                        buttons(ui);
                                    }
                                }
                            }
                        });
                });
            if popup.is_some_and(|p| p.response.clicked_elsewhere())
                && ctx.input(|i| i.pointer.primary_clicked())
            {
                close = true;
            }
            if close {
                self.menu = None;
            }
        }
        if let Some((item, from)) = self.drag {
            if f.inventory.at(from) != Some(item) {
                self.drag = None;
            } else {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::unique("drag-item"),
                ));
                let pointer = ctx.input(|i| i.pointer.interact_pos());
                for (command, rect) in &self.targets {
                    let to = match *command {
                        Command::Slot(slot) => slot,
                        Command::Hand(hand) => 16 + hand,
                        _ => continue,
                    };
                    let allowed = f.inventory.transfer_allowed(item, from, to);
                    if allowed {
                        painter.rect_stroke(
                            *rect,
                            0.,
                            egui::Stroke::new(2., egui::Color32::LIGHT_GREEN),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if pointer.is_some_and(|p| rect.contains(p))
                        && ctx.input(|i| i.pointer.primary_released())
                        && allowed
                    {
                        commands.push(Command::Transfer { item, from, to });
                    }
                }
                if let Some(p) = pointer
                    && let Some(Some((texture, uv, _))) = icons.get(item)
                {
                    painter.image(
                        *texture,
                        egui::Rect::from_center_size(p, egui::vec2(40., 40.)),
                        *uv,
                        egui::Color32::WHITE,
                    );
                }
                if ctx.input(|i| i.pointer.primary_released() || !i.pointer.primary_down()) {
                    self.drag = None;
                }
            }
        }
        commands
    }
}
