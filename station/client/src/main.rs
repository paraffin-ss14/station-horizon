mod editor;
mod locomotion;
mod motion;
mod object_menu;
mod power_smoke;
mod sprite_icons;
mod sprites;
mod ui;
mod ui_smoke;
mod ui_visuals;
use horizon_station_render::{RenderItem, Renderer};
use station_server::{Input, net::ClientSession as Session};
use std::{collections::HashSet, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
struct App {
    motion: motion::Motion,
    locomotion: locomotion::Locomotion,
    camera: Option<[f32; 3]>,
    epoch: u64,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    icons: sprite_icons::Icons,
    session: Session,
    keys: HashSet<KeyCode>,
    smoke: bool,
    smoke_job: Option<station_server::Job>,
    smoke_visibility: bool,
    smoke_sprites: bool,
    smoke_tiles: bool,
    smoke_power: bool,
    smoke_power_observer: bool,
    power_phase: u8,
    power_click_tick: u64,
    tile_phase: u8,
    sprite_phase: u8,
    walked: bool,
    visibility_phase: u8,
    hole: bool,
    sent: bool,
    start: Instant,
    ctx: egui::Context,
    input: Option<egui_winit::State>,
    ui: ui::Ui,
    cursor: [f32; 2],
    smoke_ui: bool,
    shell_smoke: Option<ui_smoke::Smoke>,
    smoke_storage: bool,
    smoke_actions: bool,
    ui_phase: usize,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let w = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes().with_inner_size(winit::dpi::LogicalSize::new(1100.,760.))
                        .with_title("Station — WASD / F floor / Space jump / G gravity / R reset / E pickup / Q drop / T throw"),
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
        let renderer = Renderer::new(w.clone());
        if !renderer.shader_pack_status.is_empty() {
            self.ui.sprite_message = format!("{}: {}", ui::tr("shader.status"), renderer.shader_pack_status);
        }
        self.renderer = Some(renderer);
        self.window = Some(w);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let consumed = self
            .input
            .as_mut()
            .unwrap()
            .on_window_event(self.window.as_ref().unwrap(), &event)
            .consumed;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = [position.x as f32, position.y as f32]
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if !consumed => {
                let f = self.session.frame();
                let epoch = f.round.map_or(0, |r| r.epoch);
                if epoch != self.epoch {
                    self.keys.clear();
                    self.epoch = epoch;
                }
                let size = self.window.as_ref().unwrap().inner_size();
                let camera = self.camera.unwrap_or(f.player.position);
                let world = [
                    camera[0]
                        + (self.cursor[0] - size.width as f32 / 2.) * 18. / size.height as f32,
                    camera[1]
                        - (self.cursor[1] - size.height as f32 / 2.) * 18. / size.height as f32,
                ];
                let deck = if f.player.position[2] >= 0.5 { 0. } else { -4. };
                let cell = [
                    world[0].floor() as i32,
                    world[1].floor() as i32,
                    deck as i32,
                ];
                if button == winit::event::MouseButton::Right {
                    let renderer = self.renderer.as_ref().unwrap();
                    let targets = object_menu::pick(
                        &f,
                        world,
                        deck as i32,
                        |p| renderer.is_visible(p),
                        |item| {
                            renderer.hit_sprite(
                                item.entity.map_or([1, 0, item.id as u64, 0], |e| {
                                    [1, e.world, e.slot as u64, e.generation as u64]
                                }),
                                world,
                            )
                        },
                    );
                    let scale = self.window.as_ref().unwrap().scale_factor() as f32;
                    self.ui.objects.open(
                        targets,
                        egui::pos2(self.cursor[0] / scale, self.cursor[1] / scale),
                    );
                    self.ui.menu = None;
                    self.ui.device = None;
                    self.ui.floor = self.ui.objects.entries.is_empty().then_some(cell);
                    return;
                }
                if matches!(
                    button,
                    winit::event::MouseButton::Left | winit::event::MouseButton::Right
                ) && (f
                    .doors
                    .iter()
                    .any(|d| d.cell == [cell[0], cell[1], cell[2] + 1] && d.reachable)
                    || f.power
                        .sources
                        .iter()
                        .chain(&f.power.controllers)
                        .any(|n| n.cell == cell && n.reachable))
                {
                    self.ui.device = f
                        .doors
                        .iter()
                        .find(|d| d.cell == [cell[0], cell[1], cell[2] + 1] && d.reachable)
                        .map_or(Some(cell), |d| Some(d.cell));
                    self.ui.menu = None;
                    self.ui.floor = None;
                    return;
                }
                let target = f
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| {
                        i.visible
                            && self.renderer.as_ref().unwrap().is_visible(i.position)
                            && i.position[2] - i.half <= deck + 3.
                            && i.position[2] >= deck
                            && if i.layers.is_empty() {
                                (i.position[0] - world[0]).abs() <= i.half + 0.15
                                    && (i.position[1] - world[1]).abs() <= i.half + 0.15
                            } else {
                                self.renderer.as_ref().unwrap().hit_sprite(
                                    i.entity.map_or([1, 0, i.id as u64, 0], |e| {
                                        [1, e.world, e.slot as u64, e.generation as u64]
                                    }),
                                    world,
                                )
                            }
                    })
                    .max_by(|(_, a), (_, b)| {
                        a.sprite_style
                            .draw_depth
                            .cmp(&b.sprite_style.draw_depth)
                            .then_with(|| b.position[1].total_cmp(&a.position[1]))
                            .then_with(|| {
                                a.entity
                                    .map(|e| (e.world, e.slot, e.generation))
                                    .cmp(&b.entity.map(|e| (e.world, e.slot, e.generation)))
                            })
                            .then_with(|| a.id.cmp(&b.id))
                    })
                    .map(|(id, _)| id);
                if let Some(target) = target {
                    if button == winit::event::MouseButton::Right {
                        let scale = self.window.as_ref().unwrap().scale_factor() as f32;
                        self.ui.menu = Some((
                            target,
                            egui::pos2(self.cursor[0] / scale, self.cursor[1] / scale),
                        ));
                    } else if button == winit::event::MouseButton::Left {
                        self.submit(Input {
                            command: Some(station_server::Command::Verb {
                                target,
                                kind: f.inventory.hands[f.inventory.active].map_or(
                                    station_server::VerbKind::Pickup,
                                    |used| {
                                        if target == 1 && f.storage.locked {
                                            station_server::VerbKind::Pry(used)
                                        } else {
                                            station_server::VerbKind::UseOn(used)
                                        }
                                    },
                                ),
                            }),
                            ..Default::default()
                        });
                    }
                } else {
                    self.ui.menu = None;
                    let cell = [
                        world[0].floor() as i32,
                        world[1].floor() as i32,
                        deck as i32,
                    ];
                    if button == winit::event::MouseButton::Right {
                        self.ui.floor = Some(cell);
                    } else if button == winit::event::MouseButton::Left
                        && let Some(tool) = f.inventory.hands[f.inventory.active]
                    {
                        self.submit(Input {
                            command: Some(station_server::Command::Work {
                                target: ui::floor_target(cell, f.items[tool].kind),
                                tool,
                            }),
                            ..Default::default()
                        });
                    }
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => self.renderer.as_mut().unwrap().resize(s.width, s.height),
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.submit(Input {
                    command: self
                        .session
                        .frame()
                        .action
                        .map(|a| station_server::Command::CancelWork(a.id)),
                    ..Default::default()
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    if event.state == ElementState::Pressed
                        && !event.repeat
                        && key == KeyCode::F5
                        && let Some(path) = std::env::var_os("STATION_SPRITE_DIR")
                    {
                        let renderer = self.renderer.as_mut().unwrap();
                        match renderer.reload_sprites(std::path::Path::new(&path)) {
                            Ok(()) => {
                                self.icons.clear();
                                self.ui.sprite_message = ui::tr("sprites.reload_done").into();
                            }
                            Err(error) => {
                                self.ui.sprite_message =
                                    format!("{}: {error}", ui::tr("sprites.reload_error"));
                            }
                        }
                    }
                    if event.state == ElementState::Pressed {
                        self.keys.insert(key);
                    } else {
                        self.keys.remove(&key);
                    }
                    if event.state == ElementState::Pressed && !event.repeat {
                        if key == KeyCode::Escape {
                            self.ui.menu = None;
                            self.ui.inventory = false;
                            self.ui.floor = None;
                        }
                        if key == KeyCode::KeyI {
                            self.ui.inventory = !self.ui.inventory;
                        }
                        let command = match key {
                            KeyCode::Escape => self
                                .session
                                .frame()
                                .action
                                .map(|a| station_server::Command::CancelWork(a.id)),
                            KeyCode::KeyF => {
                                let f = self.session.frame();
                                let epoch = f.round.map_or(0, |r| r.epoch);
                                if epoch != self.epoch {
                                    self.keys.clear();
                                    self.epoch = epoch;
                                }
                                f.inventory.hands[f.inventory.active].map(|tool| {
                                    station_server::Command::Work {
                                        target: ui::floor_target(
                                            [
                                                f.player.position[0].floor() as i32,
                                                f.player.position[1].floor() as i32,
                                                if f.player.position[2] >= 0.5 { 0 } else { -4 },
                                            ],
                                            f.items[tool].kind,
                                        ),
                                        tool,
                                    }
                                })
                            }
                            KeyCode::KeyX => Some(station_server::Command::Swap),
                            KeyCode::KeyZ => self.session.frame().inventory.hands
                                [self.session.frame().inventory.active]
                                .map(|target| station_server::Command::Verb {
                                    target,
                                    kind: station_server::VerbKind::Activate,
                                }),
                            _ => None,
                        };
                        self.submit(Input {
                            command,
                            movement: self.movement(),
                            open_floor: false,
                            jump: key == KeyCode::Space,
                            toggle_gravity: key == KeyCode::KeyG,
                            reset: key == KeyCode::KeyR,
                            pickup: key == KeyCode::KeyE,
                            drop_item: key == KeyCode::KeyQ,
                            throw_item: key == KeyCode::KeyT,
                            ..Default::default()
                        });
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let mut f = self.session.frame();
                let epoch = f.round.map_or(0, |r| r.epoch);
                if epoch != self.epoch {
                    self.keys.clear();
                    self.epoch = epoch;
                }
                if self.smoke
                    && f.local_id != 0
                    && f.round
                        .is_some_and(|r| r.phase == station_server::Phase::Lobby && !r.ready)
                {
                    self.submit(Input {
                        round_command: Some(
                            if let Some(job) = self.smoke_job
                                && f.crew.as_ref().is_some_and(|c| c.selected != job)
                            {
                                station_server::RoundCommand::Job(job)
                            } else {
                                station_server::RoundCommand::Ready(true)
                            },
                        ),
                        ..Default::default()
                    });
                }
                if self.smoke_sprites && self.smoke_job == Some(station_server::Job::Doctor) {
                    if (180..190).contains(&f.player.tick) {
                        self.keys.insert(KeyCode::KeyA);
                    } else {
                        self.keys.remove(&KeyCode::KeyA);
                    }
                }
                let view = self.motion.update(&f, self.movement(), Instant::now());
                self.camera = Some(view.player);
                let renderer = self.renderer.as_mut().unwrap();
                renderer.set_floors(sprites::floors(&f.floors));
                renderer.set_barriers(sprites::barriers(&f.barriers));
                renderer.lighting_enabled = true;
                renderer.restrict_view = true;
                renderer.lights = f
                    .lamps
                    .iter()
                    .map(|l| horizon_station_render::Light {
                        position: l.position,
                        radius: l.radius,
                        color: l.color,
                        energy: l.energy,
                    })
                    .collect();
                renderer.prepare_visibility(&f.cells, view.player);
                for i in &mut f.items {
                    if i.visible && !renderer.is_visible(i.position) {
                        i.visible = false;
                        if self.ui.menu.as_ref().is_some_and(|(id, _)| *id == i.id) {
                            self.ui.menu = None;
                        }
                    }
                }

                self.renderer.as_mut().unwrap().character_size = f.character.sprite_size;
                self.renderer.as_mut().unwrap().sprite_time = view.time;
                self.locomotion.retain(
                    epoch,
                    &f.appearances.iter().map(|a| a.id).collect::<Vec<_>>(),
                );
                let input_moving = self.movement().iter().any(|v| v.abs() > 0.01);
                self.renderer.as_mut().unwrap().actors = f
                    .appearances
                    .iter()
                    .filter_map(|a| {
                        let position = if a.id == f.local_id {
                            view.player
                        } else {
                            view.peers.iter().find(|(id, _)| *id == a.id)?.1
                        };
                        let (_, facing, moving) = view
                            .poses
                            .iter()
                            .find(|p| p.0 == a.id)
                            .copied()
                            .unwrap_or((a.id, a.facing, a.moving));
                        let mut style = sprites::style(&a.style);
                        self.locomotion.apply(
                            a.id,
                            position,
                            if a.id == f.local_id {
                                input_moving
                            } else {
                                moving
                            },
                            view.time,
                            &mut style,
                        );
                        if a.id == f.local_id && style.offset[1] > a.style.offset[1] + 0.001 {
                            self.walked = true;
                        }
                        Some(horizon_station_render::RenderActor {
                            id: [0, a.id, 0, 0],
                            style,
                            position,
                            facing,
                            layers: sprites::layers(&a.layers),
                        })
                    })
                    .collect();
                self.ui.sprite_icons =
                    self.icons
                        .update(&self.ctx, &f, &self.renderer.as_ref().unwrap().atlas);
                self.ui.portraits =
                    self.icons
                        .portraits(&self.ctx, &f, &self.renderer.as_ref().unwrap().atlas);
                let mut items: Vec<_> = f
                    .items
                    .iter()
                    .filter(|i| i.visible && !i.held_layer)
                    .map(|i| RenderItem {
                        id: i.entity.map_or([1, 0, i.id as u64, 0], |e| {
                            [1, e.world, e.slot as u64, e.generation as u64]
                        }),
                        style: sprites::style(&i.sprite_style),
                        layers: sprites::layers(&i.layers),
                        sprite_size: i.sprite_size,
                        position: i.position,
                        rotation: i.rotation,
                        half: i.half,
                        held: i.held,
                        color: i.color,
                        billboard: i.billboard,
                    })
                    .collect();
                items.extend(sprites::power_items(&f.power));
                items.extend(f.doors.iter().map(|d| {
                    sprites::door(
                        [
                            3,
                            d.entity.world,
                            d.entity.slot as u64,
                            d.entity.generation as u64,
                        ],
                        d.cell,
                        d.glass,
                        d.horizontal,
                        d.phase,
                        d.elapsed as f32 / 60.
                            + if d.phase.moving() {
                                (view.time - f.player.tick as f64 / 60.).clamp(0., 0.1) as f32
                            } else {
                                0.
                            },
                    )
                }));
                self.renderer
                    .as_mut()
                    .unwrap()
                    .set_doors(f.doors.iter().map(|d| d.cell).collect());
                if let Some(job) = self.smoke_job
                    && !self.smoke_power
                {
                    self.ui.device = f.doors.get(job.index()).map(|d| d.cell);
                }
                if self.smoke_job.is_some() {
                    self.ui.inventory = self.ui_phase > 0;
                }
                if self.smoke_ui {
                    self.ui.inventory = true;
                    self.ui.menu = Some((0, egui::pos2(360., 40.)));
                }
                if self.smoke_storage {
                    self.ui.menu = Some((
                        match self.ui_phase {
                            0 => 3,
                            4 | 7 => 0,
                            _ => 1,
                        },
                        egui::pos2(360., 40.),
                    ));
                }
                if self.smoke_actions {
                    self.ui.floor = Some([2, 1, 0]);
                    self.ui.menu = if self.ui_phase == 0 {
                        Some((3, egui::pos2(360., 40.)))
                    } else {
                        None
                    };
                }
                let tile_cell = [
                    f.player.position[0].floor() as i32,
                    f.player.position[1].floor() as i32,
                    0,
                ];
                if self.smoke_tiles && f.player.tick >= 150 {
                    self.ui.inventory = matches!(self.tile_phase, 0 | 3 | 6);
                    self.ui.floor = (!self.ui.inventory).then_some(tile_cell);
                }
                if self.smoke_power {
                    self.ui.device = f.doors.first().map(|d| d.cell);
                    power_smoke::panels(self.power_phase, &mut self.ui);
                }
                let mut raw = self
                    .input
                    .as_mut()
                    .unwrap()
                    .take_egui_input(self.window.as_ref().unwrap());
                if let Some(smoke) = &mut self.shell_smoke
                    && smoke.step(&mut self.ui, &f, &mut raw, self.renderer.as_mut().unwrap())
                {
                    event_loop.exit();
                    return;
                }
                if self.smoke_tiles && f.player.tick >= 150 {
                    use station_server::{Command, WorkTarget};
                    use station_sim::{ItemKind, floor::FloorKind};
                    let kind = f
                        .floors
                        .iter()
                        .find(|t| t.cell == tile_cell)
                        .map_or(FloorKind::Steel, |t| t.kind);
                    let wanted = match self.tile_phase {
                        0 => Some(Command::Slot(11)),
                        1 => f.inventory.hands[0]
                            .filter(|&i| f.items[i].kind == ItemKind::Crowbar)
                            .map(|tool| Command::Work {
                                target: WorkTarget::LiftTile(tile_cell),
                                tool,
                            }),
                        2 if f.action.is_none() && kind == FloorKind::Plating => {
                            Some(Command::Hand(1))
                        }
                        3 if f.inventory.active == 1 => Some(Command::Slot(12)),
                        4 => f.inventory.hands[1]
                            .filter(|&i| matches!(f.items[i].kind, ItemKind::Tile(_)))
                            .map(|tool| Command::Work {
                                target: WorkTarget::LayTile(tile_cell),
                                tool,
                            }),
                        5 if f.action.is_none() && kind == FloorKind::Steel => {
                            self.tile_phase = 6;
                            None
                        }
                        _ => None,
                    };
                    if let Some(wanted) = wanted
                        && let Some((_, rect)) = self.ui.targets.iter().find(|(c, _)| *c == wanted)
                    {
                        let pos = rect.center();
                        raw.events.push(egui::Event::PointerMoved(pos));
                        for pressed in [true, false] {
                            raw.events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            });
                        }
                        self.tile_phase += 1;
                    }
                }
                if self.smoke_power {
                    power_smoke::drive(
                        &mut self.power_phase,
                        &mut self.power_click_tick,
                        &f,
                        &self.ui,
                        &mut raw,
                        self.renderer.as_mut().unwrap(),
                    );
                }
                if let Some(job) = self.smoke_job
                    && !self.smoke_power
                    && self.ui_phase == 0
                    && f.player.tick >= 120
                    && let Some((_, rect)) = self
                        .ui
                        .targets
                        .iter()
                        .find(|(c, _)| *c == station_server::Command::Door(job.index()))
                {
                    let pos = rect.center();
                    raw.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        });
                    }
                    self.ui_phase = 1;
                }
                if self.smoke_ui
                    && self.ui_phase < 2
                    && f.player.tick >= 90 + 60 * self.ui_phase as u64
                {
                    let wanted = if self.ui_phase == 0 {
                        station_server::Command::Verb {
                            target: 0,
                            kind: station_server::VerbKind::Pickup,
                        }
                    } else {
                        station_server::Command::Slot(11)
                    };
                    let pos = self
                        .ui
                        .targets
                        .iter()
                        .find(|(c, _)| *c == wanted)
                        .expect("UI smoke target")
                        .1
                        .center();
                    raw.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        });
                    }
                    self.ui_phase += 1;
                }
                if self.smoke_storage
                    && self.ui_phase < 11
                    && f.player.tick
                        >= 180
                            + 45 * self.ui_phase as u64
                            + if self.ui_phase >= 2 { 150 } else { 0 }
                {
                    use station_server::{Command, VerbKind};
                    let wanted = [
                        Command::Verb {
                            target: 3,
                            kind: VerbKind::Pickup,
                        },
                        Command::Verb {
                            target: 1,
                            kind: VerbKind::Pry(3),
                        },
                        Command::Verb {
                            target: 1,
                            kind: VerbKind::Open,
                        },
                        Command::Store {
                            container: 1,
                            item: 3,
                        },
                        Command::Verb {
                            target: 0,
                            kind: VerbKind::Pickup,
                        },
                        Command::Store {
                            container: 1,
                            item: 0,
                        },
                        Command::Take {
                            container: 1,
                            item: 0,
                        },
                        Command::Verb {
                            target: 0,
                            kind: VerbKind::Activate,
                        },
                        Command::Verb {
                            target: 1,
                            kind: VerbKind::UseOn(0),
                        },
                        Command::Verb {
                            target: 1,
                            kind: VerbKind::Close,
                        },
                        Command::Verb {
                            target: 1,
                            kind: VerbKind::Open,
                        },
                    ][self.ui_phase];
                    let enabled = match wanted {
                        Command::Verb { target, kind } => {
                            f.verbs[target].iter().any(|v| v.kind == kind && v.enabled)
                        }
                        _ => true,
                    };
                    if enabled
                        && let Some((_, rect)) = self.ui.targets.iter().find(|(c, _)| *c == wanted)
                    {
                        println!(
                            "storage phase {} tick {} wanted {:?} hands {:?} storage {:?}",
                            self.ui_phase, f.player.tick, wanted, f.inventory.hands, f.storage
                        );
                        let pos = rect.center();
                        raw.events.push(egui::Event::PointerMoved(pos));
                        for pressed in [true, false] {
                            raw.events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            });
                        }
                        self.ui_phase += 1;
                    }
                }
                if self.smoke_actions {
                    use station_server::{Command, VerbKind, WorkTarget};
                    let wanted = match self.ui_phase {
                        0 if f.player.tick >= 180 => Some(Command::Verb {
                            target: 3,
                            kind: VerbKind::Pickup,
                        }),
                        1 if f.player.tick >= 240 => Some(Command::Work {
                            target: WorkTarget::Floor([2, 1, 0]),
                            tool: 3,
                        }),
                        2 => f
                            .action
                            .filter(|a| a.elapsed >= 60)
                            .map(|a| Command::CancelWork(a.id)),
                        3 if f.player.tick >= 360 && f.action.is_none() => {
                            assert_eq!(
                                f.world_revision, 0,
                                "cancel click did not stop first action"
                            );
                            assert!(f.cells.contains(&[2, 1, 0]));
                            Some(Command::Work {
                                target: WorkTarget::Floor([2, 1, 0]),
                                tool: 3,
                            })
                        }
                        _ => None,
                    };
                    if let Some(wanted) = wanted
                        && let Some((_, rect)) = self.ui.targets.iter().find(|(c, _)| *c == wanted)
                    {
                        let pos = rect.center();
                        raw.events.push(egui::Event::PointerMoved(pos));
                        for pressed in [true, false] {
                            raw.events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            });
                        }
                        self.ui_phase += 1;
                    }
                }
                if self.smoke_sprites
                    && self.sprite_phase == 0
                    && f.player.tick >= 150
                    && let Some((_, rect)) = self
                        .ui
                        .targets
                        .iter()
                        .find(|(c, _)| *c == station_server::Command::Slot(11))
                {
                    let pos = rect.center();
                    raw.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        });
                    }
                    self.sprite_phase = 1;
                }
                if self.smoke_sprites && self.sprite_phase == 1 && f.player.tick >= 235 {
                    assert!(
                        f.appearances
                            .iter()
                            .filter(|a| a.id != 0)
                            .all(|a| a.layers.iter().any(|l| l.state.contains("inhand-left"))),
                        "replicated hand layers"
                    );
                    assert!(f.appearances.iter().any(|a| {
                        a.layers
                            .iter()
                            .any(|l| l.rsi == "engineer" && l.state == "equipped-INNERCLOTHING")
                    }));
                    assert!(f.appearances.iter().any(|a| {
                        a.layers
                            .iter()
                            .any(|l| l.rsi == "doctor" && l.state == "equipped-INNERCLOTHING")
                    }));
                    println!(
                        "RSI sprites: replicated uniforms and hand layers; {} states in GPU atlas",
                        self.renderer.as_ref().unwrap().atlas.state_count()
                    );
                    self.sprite_phase = 2;
                }
                self.ui.objects.camera = view.player;
                self.ui
                    .objects
                    .retain_visible(&f, |p| self.renderer.as_ref().unwrap().is_visible(p));
                self.ui.objects.icons =
                    self.icons
                        .devices(&self.ctx, &f, &self.renderer.as_ref().unwrap().atlas);
                let mut commands = Vec::new();
                let output = self.ctx.run_ui(raw, |ctx| {
                    commands = self.ui.draw(ctx, &f);
                });
                self.input.as_mut().unwrap().handle_platform_output(
                    self.window.as_ref().unwrap(),
                    output.platform_output.clone(),
                );
                self.renderer.as_mut().unwrap().set_ui(&self.ctx, output);
                if let Some(command) = self.ui.round_command.take() {
                    self.submit(Input {
                        round_command: Some(command),
                        ..Default::default()
                    });
                }
                for command in commands {
                    if self.smoke_storage {
                        println!("UI emitted {:?}", command);
                    }
                    self.submit(Input {
                        command: Some(command),
                        ..Default::default()
                    });
                }
                let hole = self.smoke && self.hole && !self.sent && f.player.tick >= 120;
                if hole {
                    self.sent = true;
                }
                self.submit(Input {
                    movement: if (self.smoke_storage && (self.ui_phase == 0 || self.ui_phase >= 3))
                        || (self.smoke_actions
                            && (self.ui_phase == 0
                                || (self.ui_phase == 4
                                    && f.world_revision > 0
                                    && f.player.position[2] >= 0.5)))
                    {
                        std::array::from_fn(|i| {
                            let d = (if self.smoke_actions {
                                [2.5, 1.5]
                            } else if self.ui_phase == 4 {
                                [f.items[0].position[0] + 0.5, f.items[0].position[1] + 0.5]
                            } else {
                                [2.6, 1.6]
                            })[i]
                                - f.player.position[i];
                            if d.abs() < 0.04 {
                                0.
                            } else {
                                (d * 2.).clamp(-1., 1.)
                            }
                        })
                    } else if self.smoke_sprites
                        && self.smoke_job == Some(station_server::Job::Doctor)
                        && (180..190).contains(&f.player.tick)
                    {
                        [-1., 0.]
                    } else {
                        self.movement()
                    },
                    drop_item: self.smoke_actions
                        && self.ui_phase == 4
                        && f.world_revision > 0
                        && f.items[3].held,
                    open_floor: hole,
                    ..Default::default()
                });
                if self.smoke_actions && !self.sent && f.action.is_some_and(|a| a.elapsed >= 30) {
                    self.sent = true;
                    self.renderer
                        .as_mut()
                        .unwrap()
                        .capture_next("station/evidence/wgpu-action-progress.png");
                }
                if self.smoke
                    && !self.smoke_ui
                    && !self.smoke_storage
                    && !self.smoke_actions
                    && !self.hole
                    && f.local_id != 0
                    && !self.sent
                    && f.player.tick >= 240
                {
                    self.sent = true;
                    self.renderer
                        .as_mut()
                        .unwrap()
                        .capture_next("station/evidence/wgpu-network-peers.png");
                    println!(
                        "network render: {} peers, RTT {:.1} ms",
                        f.peers.len(),
                        f.rtt_ms
                    );
                }
                if self.smoke_power_observer && !f.doors.is_empty() {
                    if !f.doors[0].powered {
                        self.power_phase |= 1;
                        if f.doors[0].open {
                            self.power_phase |= 4;
                        }
                    } else if self.power_phase & 1 != 0 {
                        self.power_phase |= 2;
                    }
                }
                if self.smoke_visibility {
                    let renderer = self.renderer.as_mut().unwrap();
                    let others: Vec<_> =
                        f.peers.iter().filter(|(id, _)| *id != f.local_id).collect();
                    if self.visibility_phase == 0 && f.player.tick >= 90 {
                        assert!(!others.is_empty(), "visibility smoke requires two peers");
                        assert!(
                            others.iter().all(|(_, p)| !renderer.is_visible(*p)),
                            "closed doors must hide other crew"
                        );
                        renderer.capture_next("station/evidence/visibility-closed.png");
                        println!("visibility: other crew hidden behind closed doors");
                        self.visibility_phase = 1;
                    }
                    if f.doors.iter().any(|d| {
                        d.phase == station_sim::crew::DoorPhase::Opening
                            && (6..24).contains(&d.elapsed)
                    }) {
                        renderer.capture_next("station/evidence/visibility-opening.png");
                    }
                    if self.visibility_phase == 1 && f.player.tick >= 240 {
                        assert!(
                            others.iter().any(|(_, p)| renderer.is_visible(*p)),
                            "open doors must reveal other crew"
                        );
                        renderer.capture_next("station/evidence/visibility-open.png");
                        println!(
                            "visibility: other crew visible through opened doors; lamps={}",
                            f.lamps.len()
                        );
                        self.visibility_phase = 2;
                    }
                }
                self.renderer.as_mut().unwrap().draw_with_peers(
                    &f.cells,
                    view.player,
                    &items,
                    &view
                        .peers
                        .iter()
                        .filter(|(id, _)| *id != f.local_id)
                        .map(|(_, p)| *p)
                        .collect::<Vec<_>>(),
                );
                if self.smoke
                    && f.player.tick
                        >= if self.smoke_power || self.smoke_power_observer {
                            960
                        } else if self.smoke_tiles {
                            450
                        } else if self.smoke_storage {
                            930
                        } else if self.smoke_actions {
                            720
                        } else {
                            300
                        }
                {
                    assert!(
                        self.smoke_storage
                            || self.smoke_actions
                            || (f.player.position[2] - if self.hole { -2.7 } else { 1.3 }).abs()
                                < 0.05
                    );
                    if self.smoke_sprites {
                        assert_eq!(self.sprite_phase, 2);
                        if self.smoke_job == Some(station_server::Job::Doctor) {
                            assert!(self.walked, "walking presentation was never exercised");
                            println!(
                                "locomotion: distance-driven walk exercised with equipped layers"
                            );
                        }
                    }
                    if self.smoke_power_observer {
                        assert_eq!(
                            self.power_phase, 7,
                            "observer must see outage, manual opening and restored supply"
                        );
                        println!(
                            "power observer passed: replicated outage, manual opening and restoration"
                        );
                    }
                    if self.smoke_power {
                        assert_eq!(
                            self.power_phase, 12,
                            "power UI phase {}, action {:?}, feedback {}",
                            self.power_phase, f.action, f.inventory.message
                        );
                        assert_eq!(f.power.revision, 2);
                        println!(
                            "power GUI smoke passed: lift, disconnect, dark room, manual pry, reconnect; two clients"
                        );
                    }
                    if self.smoke_tiles {
                        assert_eq!(
                            self.tile_phase, 6,
                            "tile UI flow incomplete: {}",
                            f.inventory.message
                        );
                        let tile = f.inventory.hands[1].unwrap();
                        assert_eq!(f.items[tile].stack_count, 10);
                        assert!(f.cells.contains(&tile_cell));
                        assert_eq!(f.world_revision, 4); // two doors plus lift and lay
                        println!(
                            "tile GUI smoke passed: lift, returned material, lay, count=10, deck intact"
                        );
                    }
                    if self.smoke_power_observer && !f.doors.is_empty() {
                        if !f.doors[0].powered {
                            self.power_phase |= 1;
                            if f.doors[0].open {
                                self.power_phase |= 4;
                            }
                        } else if self.power_phase & 1 != 0 {
                            self.power_phase |= 2;
                        }
                    }
                    if self.smoke_visibility {
                        assert_eq!(self.visibility_phase, 2);
                    }
                    if f.round.is_none() {
                        assert_eq!(f.items.len(), 4);
                    } else {
                        assert!(f.items.len() >= 5);
                        assert!(f.inventory.slots[10].is_some());
                    }
                    if let Some(job) = self.smoke_job {
                        assert_eq!(f.crew.as_ref().unwrap().selected, job);
                        if self.smoke_sprites || self.smoke_tiles {
                            assert!(f.inventory.slots[11].is_none());
                            assert!(f.inventory.hands[0].is_some());
                        } else {
                            assert!(f.inventory.slots[11].is_some());
                        }
                        assert!(f.doors[job.index()].open, "door UI did not open the door");
                    }
                    assert!(
                        self.smoke_ui
                            || self.smoke_actions
                            || self.smoke_storage
                            || (f.items[0].position[2] - if self.hole { -2.72 } else { 1.28 })
                                .abs()
                                < 0.08,
                        "crate did not land: {:?}",
                        f.items[0]
                    );
                    if self.smoke_ui {
                        assert_eq!(
                            f.inventory.slots[11],
                            Some(0),
                            "UI clicks did not equip pocket"
                        );
                        assert_eq!(f.inventory.hands, [None, None]);
                        assert!(!f.items[0].visible);
                    }
                    if self.smoke_storage {
                        assert_eq!(
                            self.ui_phase, 11,
                            "storage smoke: {:?}, action {:?}, feedback {}",
                            f.storage, f.action, f.inventory.message
                        );
                        assert!(f.storage.open && !f.storage.locked && f.storage.accessible);
                        assert_eq!(f.storage.items, vec![3, 0]);
                        assert!(f.items[0].activated && !f.items[0].visible);
                        assert_eq!(f.inventory.hands, [None, None]);
                        assert!(!f.items[3].visible);
                    }
                    if self.smoke_actions {
                        assert_eq!(self.ui_phase, 4);
                        assert!(self.sent);
                        assert!(f.action.is_none());
                        assert!(!f.cells.contains(&[2, 1, 0]));
                        assert_eq!(f.world_revision, 1);
                        assert!(
                            (f.player.position[2] + 2.7).abs() < 0.05,
                            "floor collider did not update: {:?}",
                            f.player
                        );
                    }
                    let renderer = self.renderer.as_mut().unwrap();
                    std::fs::create_dir_all("station/evidence").unwrap();
                    renderer.capture_next(if self.smoke_actions {
                        "station/evidence/wgpu-action-complete.png"
                    } else if self.smoke_storage {
                        "station/evidence/wgpu-storage.png"
                    } else if self.smoke_ui {
                        "station/evidence/wgpu-inventory.png"
                    } else if self.hole {
                        "station/evidence/wgpu-lower.png"
                    } else {
                        "station/evidence/wgpu-upper.png"
                    });
                    renderer.draw(&f.cells, f.player.position, &items);
                    let uploaded = renderer.terrain_uploads;
                    for offset in [0.1, 0.5, 1.0] {
                        renderer.draw(
                            &f.cells,
                            [
                                f.player.position[0] + offset,
                                f.player.position[1],
                                f.player.position[2],
                            ],
                            &items,
                        );
                    }
                    assert_eq!(
                        renderer.terrain_uploads, uploaded,
                        "camera motion uploaded terrain"
                    );
                    renderer.capture_next(if self.hole {
                        "station/evidence/wgpu-hole.png"
                    } else {
                        "station/evidence/wgpu-wall.png"
                    });
                    renderer.draw(
                        &f.cells,
                        if self.hole {
                            [0.5, 0.5, 1.3]
                        } else {
                            [7.9, 0.5, 1.3]
                        },
                        &items,
                    );
                    println!(
                        "wgpu smoke passed: {:?}; terrain uploads during camera motion = 0",
                        f.player
                    );
                    event_loop.exit();
                }
                assert!(
                    !self.smoke || self.start.elapsed().as_secs() < 60,
                    "smoke timeout"
                );
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
impl App {
    fn submit(&self, mut input: Input) {
        input.epoch = self.epoch;
        self.session.submit(input);
    }

    fn movement(&self) -> [f32; 2] {
        [
            (self.keys.contains(&KeyCode::KeyD) as i32 - self.keys.contains(&KeyCode::KeyA) as i32)
                as f32,
            (self.keys.contains(&KeyCode::KeyW) as i32 - self.keys.contains(&KeyCode::KeyS) as i32)
                as f32,
        ]
    }
}
fn main() {
    let ctx = egui::Context::default();
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Geist".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../crates/horizon-theme/fonts/Geist-Regular.ttf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "Geist".into());
    }
    ctx.set_fonts(fonts);
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--editor") {
        editor::run(ctx, &args);
        return;
    }

    let mut app = App {
        motion: motion::Motion::default(),
        locomotion: locomotion::Locomotion::default(),
        camera: None,
        window: None,
        renderer: None,
        icons: Default::default(),
        epoch: if std::env::args().any(|a| a == "--smoke") {
            0
        } else {
            1
        },
        session: if let Some(address) = std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|a| a[0] == "--connect")
            .map(|a| a[1].clone())
        {
            Session::connect(&address).expect("connect station server")
        } else if std::env::args().any(|a| a == "--smoke") {
            Session::start(7)
        } else {
            let content = std::env::args()
                .collect::<Vec<_>>()
                .windows(2)
                .find(|a| a[0] == "--content")
                .map_or_else(
                    || Ok(station_server::Content::test_station()),
                    |a| station_server::Content::load(&a[1]),
                )
                .expect("load station content");
            Session::start_content(7, content)
        },
        keys: HashSet::new(),
        smoke: std::env::args().any(|a| a == "--smoke"),
        smoke_visibility: std::env::args().any(|a| a == "--smoke-visibility"),
        smoke_tiles: std::env::args().any(|a| a == "--smoke-tiles"),
        smoke_power: std::env::args().any(|a| a == "--smoke-power"),
        smoke_power_observer: std::env::args().any(|a| a == "--smoke-power-observer"),
        power_phase: 0,
        power_click_tick: 0,
        tile_phase: 0,
        smoke_sprites: std::env::args().any(|a| a == "--smoke-sprites"),
        sprite_phase: 0,
        walked: false,
        visibility_phase: 0,
        smoke_job: std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|a| a[0] == "--smoke-job")
            .map(|a| match a[1].as_str() {
                "engineer" => station_server::Job::Engineer,
                "doctor" => station_server::Job::Doctor,
                _ => station_server::Job::Assistant,
            }),
        hole: std::env::args().any(|a| a == "--smoke-hole"),
        sent: false,
        start: Instant::now(),
        ctx,
        input: None,
        ui: ui::Ui::default(),
        cursor: [0.; 2],
        ui_phase: 0,
        smoke_ui: std::env::args().any(|a| a == "--smoke-ui"),
        shell_smoke: std::env::args()
            .any(|a| a == "--smoke-shell")
            .then(ui_smoke::Smoke::default),
        smoke_storage: std::env::args().any(|a| a == "--smoke-storage"),
        smoke_actions: std::env::args().any(|a| a == "--smoke-actions"),
    };
    EventLoop::new().unwrap().run_app(&mut app).unwrap();
}

#[cfg(test)]
mod lighting_tests {
    use horizon_station_render::station_light::Occlusion;
    use station_sim::{Simulation, crew::Job, interaction::Command};
    #[test]
    fn actual_door_and_wall_edits_change_visibility_without_changing_other_deck() {
        let mut sim = Simulation::new(7);
        sim.prepare_departments();
        sim.outfit_actor(0, Job::Engineer, false);
        for _ in 0..120 {
            sim.step([0.; 2]);
        }
        let from = [-4., 2.5, 1.3];
        let to = [-1.5, 2.5, 1.3];
        let view =
            |s: &Simulation, deck| Occlusion::new(&s.world().cells().collect::<Vec<_>>(), deck);
        assert!(!view(&sim, 0).entity_visible(from, to));
        sim.command(Command::Door(0));
        for _ in 0..station_sim::crew::DOOR_TICKS {
            sim.step([0.; 2]);
        }
        assert!(view(&sim, 0).entity_visible(from, to));
        sim.command(Command::Door(0));
        for _ in 0..station_sim::crew::DOOR_TICKS {
            sim.step([0.; 2]);
        }
        assert!(!view(&sim, 0).entity_visible(from, to));
        sim.edit_voxels(&[([-3, 2, 1], false), ([-3, 2, 2], false)]);
        assert!(view(&sim, 0).entity_visible(from, to));
        sim.edit_voxels(&[([-3, 2, -3], true)]);
        assert!(view(&sim, 0).entity_visible(from, to));
        assert!(!view(&sim, -4).entity_visible([-4., 2.5, -2.7], [-1.5, 2.5, -2.7]));
    }
}

#[cfg(test)]
mod airlock_tests {
    #[test]
    fn closed_glass_door_transmits_view_and_illumination() {
        use horizon_station_render::station_light::{Light, Occlusion};
        let mut content = (*station_sim::content::Content::builtin()).clone();
        content.map.doors[0].glass = true;
        let mut sim = station_sim::Simulation::with_content(7, std::sync::Arc::new(content));
        sim.prepare_departments();
        let cells = sim.world().cells().collect::<Vec<_>>();
        let view = Occlusion::with_barriers(
            &cells,
            0,
            &crate::sprites::barriers(&sim.content().map.effective_barriers()),
        );
        let from = [-4., 2.5, 1.3];
        let to = [-1.5, 2.5, 1.3];
        assert!(view.entity_visible(from, to));
        assert!(view.light_visible([from[0], from[1]], [to[0], to[1]]));
        let lamp = Light {
            position: from,
            radius: 6.,
            color: [1.; 3],
            energy: 1.,
        };
        assert!(view.illumination([to[0], to[1]], &[lamp])[0] > 0.12);
        assert!(sim.world().blocks_movement([-3, 2, 1]));
        let opaque = Occlusion::new(&cells, 0);
        assert!(!opaque.entity_visible(from, to));
        assert!(!opaque.light_visible([from[0], from[1]], [to[0], to[1]]));
    }
}
