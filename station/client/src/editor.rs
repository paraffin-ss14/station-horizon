use crate::ui::tr;
use horizon_station_render::{RenderItem, Renderer};
use station_sim::{
    Cell,
    content::Content,
    editor::{Editor, Tool},
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

pub fn run(ctx: egui::Context, args: &[String]) {
    let arg = |key: &str| args.windows(2).find(|v| v[0] == key).map(|v| v[1].clone());
    let source = arg("--content");
    let content = source
        .as_ref()
        .map_or_else(
            || {
                Ok(if arg("--editor-smoke").is_some() {
                    Content::builtin()
                } else {
                    Content::test_station()
                })
            },
            Content::load,
        )
        .expect("load editor template");
    let mut preview_sim = station_sim::Simulation::with_content(7, content.clone());
    preview_sim.prepare_departments();
    let editor = Editor::new(content);
    let cells = editor.cells();
    let mut app = App {
        rooms_preview: preview_sim.rooms(),
        show_rooms: args.iter().any(|a| a == "--rooms-preview"),
        power_preview: preview_sim.power_snapshot(),
        lamp_preview: preview_sim.powered_lamps(),
        editor,
        cells,
        revision: 0,
        ctx,
        window: None,
        renderer: None,
        input: None,
        camera: [
            0.5,
            arg("--camera-y")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.5),
            3.,
        ],
        keys: HashSet::new(),
        last: Instant::now(),
        floor_kind: Default::default(),
        floor_variant: None,
        tool: 0,
        rectangle: false,
        anchor: None,
        credits: false,
        light_radius: 6.,
        light_color: [1., 0.85, 0.65],
        light_energy: 2.,
        preview: true,
        view_preview: false,
        prototype: "flashlight".into(),
        item_key: String::new(),
        spawn: "job.assistant".into(),
        access: 0,
        door_name: "door.title".into(),
        glass: false,
        horizontal: false,
        path: arg("--map-output")
            .or(source)
            .unwrap_or_else(|| "station-map.json".into()),
        status: String::new(),
        confirm: None,
        targets: HashMap::new(),
        smoke_floor: args.iter().any(|a| a == "--editor-floor-smoke"),
        smoke_light: args.iter().any(|a| a == "--editor-light-smoke"),
        smoke_sprite: args.iter().any(|a| a == "--editor-sprite-smoke"),
        preview_capture: arg("--preview-capture"),
        preview_motion: args.iter().any(|a| a=="--preview-motion" || a=="--preview-slow-motion"),
        preview_object_motion: args.iter().any(|a| a=="--preview-object-motion"),
        preview_motion_step: if args.iter().any(|a| a=="--preview-slow-motion") {0.25} else {8.},
        smoke: arg("--editor-smoke"),
        frame: 0,
        smoke_step: 0,
        start: Instant::now(),
    };
    EventLoop::new().unwrap().run_app(&mut app).unwrap();
}
struct App {
    rooms_preview: Arc<station_sim::rooms::RoomsSnapshot>,
    show_rooms: bool,
    power_preview: station_sim::power::PowerSnapshot,
    lamp_preview: Vec<station_sim::content::Lamp>,
    editor: Editor,
    cells: Arc<Vec<Cell>>,
    revision: u64,
    ctx: egui::Context,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    input: Option<egui_winit::State>,
    camera: [f32; 3],
    keys: HashSet<KeyCode>,
    last: Instant,
    floor_kind: station_sim::floor::FloorKind,
    floor_variant: Option<u8>,
    tool: usize,
    rectangle: bool,
    anchor: Option<Cell>,
    credits: bool,
    light_radius: f32,
    light_color: [f32; 3],
    light_energy: f32,
    preview: bool,
    view_preview: bool,
    prototype: String,
    item_key: String,
    spawn: String,
    access: u8,
    door_name: String,
    glass: bool,
    horizontal: bool,
    path: String,
    status: String,
    confirm: Option<bool>,
    targets: HashMap<String, egui::Pos2>,
    preview_capture: Option<String>,
    preview_motion: bool,
    preview_motion_step: f32,
    preview_object_motion: bool,
    smoke: Option<String>,
    smoke_floor: bool,
    smoke_light: bool,
    smoke_sprite: bool,
    frame: u32,
    smoke_step: usize,
    start: Instant,
}
const TOOLS: [&str; 17] = [
    "editor.floor",
    "editor.wall",
    "editor.erase_floor",
    "editor.erase_wall",
    "editor.door",
    "editor.item",
    "editor.erase_item",
    "editor.spawn",
    "editor.lamp",
    "editor.erase_lamp",
    "editor.window",
    "editor.paint_floor",
    "power.cable",
    "power.erase_cable",
    "power.source",
    "power.controller",
    "power.erase_node",
];
impl App {
    fn tool(&self) -> Tool {
        match self.tool {
            0 => Tool::Floor,
            1 => Tool::Wall,
            2 => Tool::EraseFloor,
            3 => Tool::EraseWall,
            4 => Tool::Door {
                glass: self.glass,
                horizontal: self.horizontal,
                access: self.access,
                name: self.door_name.clone(),
            },
            5 => Tool::Item {
                prototype: self.prototype.clone(),
                key: self.item_key.clone(),
            },
            6 => Tool::EraseItem,
            7 => Tool::Spawn(self.spawn.clone()),
            8 => Tool::Lamp {
                radius: self.light_radius,
                color: self.light_color,
                energy: self.light_energy,
            },
            11 => Tool::PaintFloor {
                kind: self.floor_kind,
                variant: self.floor_variant,
            },
            12 => Tool::Cable,
            13 => Tool::EraseCable,
            14 => Tool::PowerSource,
            15 => Tool::PowerController,
            16 => Tool::ErasePowerNode,
            10 => Tool::Window,
            _ => Tool::EraseLamp,
        }
    }
    fn button(&mut self, ui: &mut egui::Ui, key: &str, enabled: bool) -> bool {
        let r = ui.add_enabled(enabled, egui::Button::new(tr(key)));
        self.targets.insert(key.into(), r.rect.center());
        r.clicked()
    }
    fn result(&mut self, result: Result<(), String>, success: &str) {
        self.status = result.err().unwrap_or_else(|| success.into());
    }
    fn ui(&mut self, root: &mut egui::Ui, event_loop: &ActiveEventLoop) {
        self.targets.clear();
        crate::sprites::credits(&self.ctx, &mut self.credits);
        egui::Panel::left("editor")
            .exact_size(285.)
            .show(root, |ui| {
                ui.heading(tr("editor.title"));
                if ui.small_button(tr("sprites.credits")).clicked() {
                    self.credits = true;
                }
                ui.label(tr(if self.editor.dirty() {
                    "editor.unsaved"
                } else {
                    "editor.saved"
                }));
                ui.text_edit_singleline(&mut self.path);
                ui.horizontal(|ui| {
                    if self.button(ui, "editor.save", true) {
                        let r = self.editor.save(&self.path);
                        self.result(r, "editor.saved");
                    }
                    if self.button(ui, "editor.load", true) {
                        if self.editor.dirty() {
                            self.confirm = Some(false);
                        } else {
                            self.load();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if self.button(ui, "editor.undo", self.editor.can_undo()) {
                        self.editor.undo();
                        self.anchor = None;
                    }
                    if self.button(ui, "editor.redo", self.editor.can_redo()) {
                        self.editor.redo();
                        self.anchor = None;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    for (label, z) in [("editor.upper", 3.), ("editor.lower", -1.)] {
                        if ui
                            .selectable_label(self.camera[2] == z, tr(label))
                            .clicked()
                        {
                            self.camera[2] = z;
                            self.anchor = None;
                        }
                    }
                });
                for (i, key) in TOOLS.iter().enumerate() {
                    let response = ui.selectable_label(self.tool == i, tr(key));
                    self.targets.insert((*key).into(), response.rect.center());
                    if response.clicked() {
                        self.tool = i;
                        self.anchor = None;
                    }
                }
                if (self.tool < 4 || [11, 12, 13].contains(&self.tool))
                    && ui
                        .checkbox(&mut self.rectangle, tr("editor.rectangle"))
                        .changed()
                {
                    self.anchor = None;
                }
                if self.tool == 11 {
                    egui::ComboBox::from_id_salt("floor-kind")
                        .selected_text(tr(self.floor_kind.label()))
                        .show_ui(ui, |ui| {
                            for kind in station_sim::floor::FloorKind::ALL {
                                if ui
                                    .selectable_value(&mut self.floor_kind, kind, tr(kind.label()))
                                    .changed()
                                {
                                    self.floor_variant = None;
                                }
                            }
                        });
                    egui::ComboBox::from_id_salt("floor-variant")
                        .selected_text(
                            self.floor_variant.map_or_else(
                                || tr("floor.auto").to_string(),
                                |v| (v + 1).to_string(),
                            ),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.floor_variant, None, tr("floor.auto"));
                            for v in 0..self.floor_kind.variants() {
                                ui.selectable_value(
                                    &mut self.floor_variant,
                                    Some(v),
                                    (v + 1).to_string(),
                                );
                            }
                        });
                }
                if self.tool == 4 {
                    ui.label(tr("editor.door_name"));
                    ui.text_edit_singleline(&mut self.door_name);
                    ui.checkbox(&mut self.glass, tr("door.glass"));
                    ui.checkbox(&mut self.horizontal, tr("door.horizontal"));
                    ui.label(tr("editor.access"));
                    for (key, mask) in [("job.engineer", 1), ("job.doctor", 2)] {
                        let mut checked = self.access & mask != 0;
                        if ui.checkbox(&mut checked, tr(key)).changed() {
                            if checked {
                                self.access |= mask;
                            } else {
                                self.access &= !mask;
                            }
                        }
                    }
                    ui.label(tr("editor.access_hint"));
                }
                if self.tool == 5 {
                    egui::ComboBox::from_id_salt("prototype")
                        .selected_text(&self.prototype)
                        .show_ui(ui, |ui| {
                            for key in self.editor.content().prototypes.keys() {
                                ui.selectable_value(&mut self.prototype, key.clone(), key);
                            }
                        });
                    ui.label(tr("editor.item_key"));
                    egui::ComboBox::from_id_salt("placement")
                        .selected_text(if self.item_key.is_empty() {
                            tr("editor.new_item")
                        } else {
                            &self.item_key
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.item_key,
                                String::new(),
                                tr("editor.new_item"),
                            );
                            for p in &self.editor.content().map.items {
                                ui.selectable_value(&mut self.item_key, p.key.clone(), &p.key);
                            }
                        });
                    ui.label(tr("editor.move_hint"));
                }
                if self.tool == 8 {
                    ui.add(
                        egui::Slider::new(&mut self.light_radius, 1.0..=16.0)
                            .text(tr("editor.light_radius")),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.light_energy, 0.0..=8.0)
                            .text(tr("editor.light_energy")),
                    );
                    ui.label(tr("editor.light_color"));
                    ui.color_edit_button_rgb(&mut self.light_color);
                    ui.label(tr("editor.light_replace"));
                }
                ui.checkbox(&mut self.preview, tr("editor.light_preview"));
                ui.checkbox(&mut self.view_preview, tr("editor.view_preview"));
                ui.checkbox(&mut self.show_rooms, tr("rooms.overlay"));
                if self.tool == 7 {
                    egui::ComboBox::from_id_salt("spawn")
                        .selected_text(tr(&self.spawn))
                        .show_ui(ui, |ui| {
                            for key in self.editor.content().map.spawns.keys() {
                                ui.selectable_value(&mut self.spawn, key.clone(), tr(key));
                            }
                        });
                }
                ui.separator();
                ui.label(tr("editor.controls"));
                if self.anchor.is_some() {
                    ui.label(tr("editor.second_corner"));
                }
                ui.label(tr(&self.status));
            });
        if let Some(exit) = self.confirm {
            egui::Window::new(tr("editor.unsaved"))
                .collapsible(false)
                .resizable(false)
                .show(&self.ctx.clone(), |ui| {
                    ui.label(tr("editor.discard_question"));
                    if ui.button(tr("editor.discard")).clicked() {
                        self.confirm = None;
                        if exit {
                            event_loop.exit();
                        } else {
                            self.load();
                        }
                    }
                    if ui.button(tr("editor.cancel")).clicked() {
                        self.confirm = None;
                    }
                });
            return;
        }
        let area = root.available_rect_before_wrap();
        let response = root.allocate_rect(area, egui::Sense::click());
        let size = self.window.as_ref().unwrap().inner_size();
        let scale = self.window.as_ref().unwrap().scale_factor() as f32;
        let screen = |x: f32, y: f32| {
            egui::pos2(
                (size.width as f32 / 2. + (x - self.camera[0]) * size.height as f32 / 18.) / scale,
                (size.height as f32 / 2. - (y - self.camera[1]) * size.height as f32 / 18.) / scale,
            )
        };
        let deck = if self.camera[2] > 0.5 { 0 } else { -4 };
        let painter = root.painter_at(area);
        if self.show_rooms {
            for room in &self.rooms_preview.rooms {
                let color = if room.space {
                    egui::Color32::from_rgba_unmultiplied(255, 65, 45, 50)
                } else {
                    egui::Color32::from_rgba_unmultiplied(45, 200, 130, 40)
                };
                for c in room.cells.iter().filter(|c| c[2] == deck) {
                    let rect = egui::Rect::from_two_pos(
                        screen(c[0] as f32, c[1] as f32),
                        screen(c[0] as f32 + 1., c[1] as f32 + 1.),
                    );
                    painter.rect_filled(rect, 0., color);
                }
                for c in room.breaches.iter().filter(|c| c[2] == deck) {
                    painter.circle_stroke(
                        screen(c[0] as f32 + 0.5, c[1] as f32 + 0.5),
                        6.,
                        egui::Stroke::new(2., egui::Color32::RED),
                    );
                }
                if let Some(c) = room.cells.iter().find(|c| c[2] == deck) {
                    painter.text(
                        screen(c[0] as f32 + 0.5, c[1] as f32 + 0.5),
                        egui::Align2::LEFT_BOTTOM,
                        format!(
                            "{}: {}",
                            room.id,
                            tr(if room.space {
                                "rooms.space"
                            } else {
                                "rooms.sealed"
                            })
                        ),
                        egui::FontId::proportional(12.),
                        egui::Color32::WHITE,
                    );
                }
            }
        }
        if let Some(power) = &self.editor.content().map.power {
            for c in power.cables.iter().filter(|c| c[2] == deck) {
                let point = screen(c[0] as f32 + 0.5, c[1] as f32 + 0.5);
                for [x, y] in [[1, 0], [0, 1]] {
                    if power.cables.contains(&[c[0] + x, c[1] + y, c[2]]) {
                        painter.line_segment(
                            [
                                point,
                                screen((c[0] + x) as f32 + 0.5, (c[1] + y) as f32 + 0.5),
                            ],
                            egui::Stroke::new(2., egui::Color32::from_rgb(220, 160, 45)),
                        );
                    }
                }
                painter.circle_filled(point, 2., egui::Color32::YELLOW);
            }
            for (source, nodes) in [(true, &power.sources), (false, &power.controllers)] {
                for n in nodes.iter().filter(|n| n.cell[2] == deck) {
                    let point = screen(n.cell[0] as f32 + 0.5, n.cell[1] as f32 + 0.5);
                    painter.text(
                        point,
                        egui::Align2::CENTER_CENTER,
                        tr(if source {
                            "power.source"
                        } else {
                            "power.controller"
                        }),
                        egui::FontId::proportional(12.),
                        egui::Color32::LIGHT_GREEN,
                    );
                }
            }
        }
        for l in &self.editor.content().map.lamps {
            if (l.position[2] >= 0.5) == (deck == 0) {
                let point = screen(l.position[0], l.position[1]);
                painter.circle_stroke(point, 5., egui::Stroke::new(2., egui::Color32::YELLOW));
                if self.tool == 8 {
                    painter.circle_stroke(
                        point,
                        l.radius * size.height as f32 / 18. / scale,
                        egui::Stroke::new(1., egui::Color32::from_rgb(150, 130, 80)),
                    );
                }
            }
        }
        for item in self
            .editor
            .content()
            .map
            .items
            .iter()
            .filter(|i| i.key.starts_with("demo."))
        {
            painter.text(
                screen(item.position[0], item.position[1] - 0.9),
                egui::Align2::CENTER_CENTER,
                tr(&item.key),
                egui::FontId::proportional(12.),
                egui::Color32::WHITE,
            );
        }
        for (key, p) in &self.editor.content().map.spawns {
            if (p[2] >= 0.5) == (deck == 0) {
                let point = screen(p[0], p[1]);
                painter.circle_stroke(point, 7., egui::Stroke::new(2., egui::Color32::LIGHT_GREEN));
                painter.text(
                    point + egui::vec2(9., 0.),
                    egui::Align2::LEFT_CENTER,
                    tr(key),
                    egui::FontId::proportional(11.),
                    egui::Color32::LIGHT_GREEN,
                );
            }
        }
        for d in &self.editor.content().map.doors {
            if d.cell[2] == deck + 1 {
                let point = screen(d.cell[0] as f32 + 0.5, d.cell[1] as f32 + 0.5);
                painter.text(
                    point,
                    egui::Align2::CENTER_CENTER,
                    format!("{} [{}]", tr(&d.name), d.access),
                    egui::FontId::proportional(11.),
                    egui::Color32::YELLOW,
                );
            }
        }
        if let Some(p) = response.hover_pos() {
            let x =
                self.camera[0] + (p.x * scale - size.width as f32 / 2.) * 18. / size.height as f32;
            let y =
                self.camera[1] - (p.y * scale - size.height as f32 / 2.) * 18. / size.height as f32;
            let cell = [x.floor() as i32, y.floor() as i32, deck];
            let a = self.anchor.unwrap_or(cell);
            let rect = egui::Rect::from_two_pos(
                screen(a[0].min(cell[0]) as f32, a[1].min(cell[1]) as f32),
                screen(a[0].max(cell[0]) as f32 + 1., a[1].max(cell[1]) as f32 + 1.),
            );
            painter.rect_stroke(
                rect,
                0.,
                egui::Stroke::new(2., egui::Color32::LIGHT_BLUE),
                egui::StrokeKind::Inside,
            );
            painter.text(
                p + egui::vec2(16., 16.),
                egui::Align2::LEFT_TOP,
                format!("{}, {} · {}", cell[0], cell[1], deck),
                egui::FontId::monospace(12.),
                egui::Color32::WHITE,
            );
            if response.clicked() {
                let result =
                    if self.rectangle && (self.tool < 4 || [11, 12, 13].contains(&self.tool)) {
                        if let Some(a) = self.anchor.take() {
                            self.editor.rectangle(a, cell, self.tool())
                        } else {
                            self.anchor = Some(cell);
                            Ok(())
                        }
                    } else {
                        self.editor.apply(&[cell], self.tool())
                    };
                self.result(result, "editor.applied");
            }
        }
    }
    fn load(&mut self) {
        let r = self.editor.load(&self.path);
        self.result(r, "editor.loaded");
        self.anchor = None;
    }
    fn smoke_events(&mut self, raw: &mut egui::RawInput) {
        if self.smoke.is_none()
            || !self.frame.is_multiple_of(15)
            || self.smoke_step >= if self.smoke_light { 8 } else { 6 }
        {
            return;
        }
        let steps = if self.smoke_light {
            vec![
                "editor.wall",
                "canvas",
                "editor.undo",
                "editor.redo",
                "editor.lamp",
                "lamp_canvas",
                "editor.save",
                "editor.load",
            ]
        } else {
            vec![
                if self.smoke_floor {
                    "editor.paint_floor"
                } else {
                    "editor.wall"
                },
                "canvas",
                "editor.undo",
                "editor.redo",
                "editor.save",
                "editor.load",
            ]
        };
        let key = steps[self.smoke_step];
        if key == "editor.paint_floor" {
            self.floor_kind = station_sim::floor::FloorKind::Damaged;
            self.floor_variant = Some(2);
        }
        if key == "editor.lamp" {
            self.light_color = [0.2, 0.5, 1.];
            self.light_energy = 4.;
        }

        let pos = if key == "canvas" || key == "lamp_canvas" {
            let w = self.window.as_ref().unwrap();
            let s = w.inner_size();
            let scale = w.scale_factor() as f32;
            egui::pos2(
                (s.width as f32 / 2.
                    + if key == "lamp_canvas" {
                        2. * s.height as f32 / 18.
                    } else {
                        0.
                    })
                    / scale,
                (s.height as f32 / 2.
                    + if key == "lamp_canvas" {
                        3. * s.height as f32 / 18.
                    } else {
                        5. * s.height as f32 / 18.
                    })
                    / scale,
            )
        } else {
            let Some(&p) = self.targets.get(key) else {
                return;
            };
            p
        };
        raw.events.push(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            raw.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        self.smoke_step += 1;
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let w = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_inner_size(winit::dpi::LogicalSize::new(1200., 800.))
                    .with_title(tr("editor.title")),
            )
            .unwrap(),
        );
        self.input = Some(egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            w.as_ref(),
            Some(w.scale_factor() as f32),
            None,
            None,
        ));
        let mut renderer = Renderer::new(w.clone());
        renderer.character_size = [0.; 2];
        self.renderer = Some(renderer);
        self.window = Some(w);
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let consumed = self
            .input
            .as_mut()
            .unwrap()
            .on_window_event(self.window.as_ref().unwrap(), &event)
            .consumed;
        match event {
            WindowEvent::CloseRequested => {
                if self.editor.dirty() {
                    self.confirm = Some(true);
                } else {
                    el.exit();
                }
            }
            WindowEvent::Resized(s) => self.renderer.as_mut().unwrap().resize(s.width, s.height),
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && event.physical_key == PhysicalKey::Code(KeyCode::F5)
                    && let Some(path) = std::env::var_os("STATION_SPRITE_DIR")
                {
                    self.status = match self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .reload_sprites(std::path::Path::new(&path))
                    {
                        Ok(()) => tr("sprites.reload_done").into(),
                        Err(e) => format!("{}: {e}", tr("sprites.reload_error")),
                    };
                }

                if let PhysicalKey::Code(key) = event.physical_key {
                    if event.state == ElementState::Released {
                        self.keys.remove(&key);
                    } else if !consumed {
                        self.keys.insert(key);
                        if key == KeyCode::Escape {
                            self.anchor = None;
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last).as_secs_f32().min(0.05);
                self.last = now;
                if self.preview_capture.is_none() && !self.ctx.egui_wants_keyboard_input() && self.confirm.is_none() {
                    self.camera[0] += (i32::from(self.keys.contains(&KeyCode::KeyD))
                        - i32::from(self.keys.contains(&KeyCode::KeyA)))
                        as f32
                        * dt
                        * 8.;
                    self.camera[1] += (i32::from(self.keys.contains(&KeyCode::KeyW))
                        - i32::from(self.keys.contains(&KeyCode::KeyS)))
                        as f32
                        * dt
                        * 8.;
                    self.camera[0] = self.camera[0].clamp(-64., 64.);
                    self.camera[1] = self.camera[1].clamp(-64., 64.);
                }
                // Reproducible motion at the capture: final camera matches the static fixture.
                if self.preview_capture.is_some() && self.preview_motion {
                    if self.frame == 19 {self.camera[0] -= self.preview_motion_step;}
                    if self.frame == 20 {self.camera[0] += self.preview_motion_step;}
                }
                let w = self.window.as_ref().unwrap().clone();
                let mut raw = self.input.as_mut().unwrap().take_egui_input(&w);
                self.smoke_events(&mut raw);
                let ctx = self.ctx.clone();
                let out = ctx.run_ui(raw, |root| self.ui(root, el));
                self.input
                    .as_mut()
                    .unwrap()
                    .handle_platform_output(&w, out.platform_output.clone());
                if self.revision != self.editor.revision {
                    self.cells = self.editor.cells();
                    let mut preview_sim = station_sim::Simulation::with_content(
                        7,
                        Arc::new(self.editor.content().clone()),
                    );
                    preview_sim.prepare_departments();
                    self.rooms_preview = preview_sim.rooms();
                    self.power_preview = preview_sim.power_snapshot();
                    self.lamp_preview = preview_sim.powered_lamps();
                    self.revision = self.editor.revision;
                }
                let mut items: Vec<_> = self
                    .editor
                    .content()
                    .map
                    .items
                    .iter()
                    .enumerate()
                    .map(|(index, p)| {
                        let proto = &self.editor.content().prototypes[&p.prototype];
                        RenderItem {
                            id: [2, 0, index as u64, 0],
                            style: crate::sprites::style(&proto.sprite.style),
                            layers: crate::sprites::layers(&proto.sprite.layers),
                            sprite_size: proto.sprite_size,
                            position: if p.key == "crate" {
                                self.editor.content().map.crate_round_position
                            } else {
                                p.position
                            },
                            rotation: [0., 0., 0., 1.],
                            half: proto.half,
                            held: false,
                            color: proto.color,
                            billboard: proto.billboard,
                        }
                    })
                    .filter(|p| (p.position[2] >= 0.5) == (self.camera[2] >= 0.5))
                    .collect();
                // Move a visible sprite for one frame, then restore the exact static scene.
                if self.preview_capture.is_some() && self.preview_object_motion && self.frame == 19
                    && let Some(item) = items.iter_mut().find(|i| i.position[0] > 0. && i.position[1] < -10.) {
                    item.position[0] -= 2.;
                    println!("TAA object fixture: shifted sprite {:?}", item.id);
                }
                items.extend(crate::sprites::power_items(&self.power_preview));
                items.extend(
                    self.editor
                        .content()
                        .map
                        .doors
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| (d.cell[2] >= 1) == (self.camera[2] >= 0.5))
                        .map(|(i, d)| {
                            crate::sprites::door(
                                [3, 0, i as u64, 0],
                                d.cell,
                                d.glass,
                                d.horizontal,
                                station_sim::crew::DoorPhase::Closed,
                                0.,
                            )
                        }),
                );
                let renderer = self.renderer.as_mut().unwrap();
                renderer.set_doors(
                    self.editor
                        .content()
                        .map
                        .doors
                        .iter()
                        .map(|d| d.cell)
                        .collect(),
                );
                if self.smoke_sprite && self.frame == 50 {
                    let path =
                        std::env::var_os("STATION_SPRITE_DIR").expect("scratch RSI directory");
                    let path = std::path::Path::new(&path);
                    let count = renderer.atlas.state_count();
                    renderer.reload_sprites(path).expect("valid RSI reload");
                    assert!(renderer.reload_sprites(&path.join("missing-pack")).is_err());
                    assert_eq!(renderer.atlas.state_count(), count);
                    println!(
                        "sprite reload: valid atlas replaced; invalid pack retained previous atlas"
                    );
                }
                renderer.sprite_time = if self.smoke_sprite {
                    self.frame as f64 / 60.
                } else {
                    self.start.elapsed().as_secs_f64()
                };
                if self.smoke_sprite
                    && [75, 90].contains(&self.frame)
                    && let Some(path) = &self.smoke
                {
                    renderer.capture_next(
                        std::path::Path::new(path)
                            .with_file_name(format!("lab-{}.png", self.frame)),
                    );
                }
                renderer.set_floors(crate::sprites::floors(&self.editor.content().map.floors));
                renderer.set_barriers(crate::sprites::barriers(
                    &self.editor.content().map.effective_barriers(),
                ));
                renderer.lighting_enabled = self.preview;
                renderer.restrict_view = self.view_preview;
                renderer.lights = self
                    .lamp_preview
                    .iter()
                    .map(|l| horizon_station_render::Light {
                        position: l.position,
                        radius: l.radius,
                        color: l.color,
                        energy: l.energy,
                    })
                    .collect();
                renderer.set_ui(&ctx, out);
                if self.smoke_step == if self.smoke_light { 8 } else { 6 }
                    && self.frame == if self.smoke_light { 135 } else { 105 }
                {
                    renderer.capture_next(self.smoke.as_ref().unwrap());
                }
                if self.frame == 20
                    && let Some(path) = &self.preview_capture
                {
                    if self.show_rooms {
                        println!(
                            "rooms preview: regions={} exposed={} cells={}",
                            self.rooms_preview.rooms.len(),
                            self.rooms_preview.rooms.iter().filter(|r| r.space).count(),
                            self.rooms_preview
                                .rooms
                                .iter()
                                .map(|r| r.cells.len())
                                .sum::<usize>()
                        );
                    }
                    renderer.capture_next(path);
                }
                renderer.draw(&self.cells, self.camera, &items);
                if self.frame == 20 && self.preview_capture.is_some() {
                    el.exit();
                }
                if self.smoke.is_some() {
                    assert!(
                        self.start.elapsed().as_secs() < 45,
                        "editor smoke timed out"
                    );
                    if self.frame == if self.smoke_light { 140 } else { 110 } {
                        assert_eq!(self.smoke_step, if self.smoke_light { 8 } else { 6 });
                        assert!(!self.editor.dirty());
                        let c = Content::load(&self.path).unwrap();
                        if self.smoke_light {
                            assert!(c.map.lamps.iter().any(|l| {
                                l.position == [2.5, -2.5, 3.]
                                    && l.color
                                        .iter()
                                        .zip([0.2, 0.5, 1.])
                                        .all(|(a, b)| (a - b).abs() < 0.00001)
                                    && l.energy == 4.
                            }));
                        }
                        let mut sim = station_sim::Simulation::with_content(7, c);
                        sim.prepare_departments();
                        if self.smoke_floor {
                            assert!(sim.floors().iter().any(|f| f.cell == [0, -5, 0]
                                && f.kind == station_sim::floor::FloorKind::Damaged
                                && f.variant == Some(2)));
                            assert!(!sim.world().solid([0, -5, 1]));
                            println!(
                                "floor GUI smoke passed: paint, undo, redo, save/load, round surface preserved"
                            );
                        } else {
                            assert!(sim.world().solid([0, -5, 1]));
                        }
                        println!(
                            "editor GUI smoke passed: place wall, undo, redo, save, load, build round template"
                        );
                        el.exit();
                    }
                }
                self.frame += 1;
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
