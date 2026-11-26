pub use station_sim::{
    content::Content,
    entity::{EntityId, EntitySnapshot},
};
pub mod crew;
pub use crew::{CrewSnapshot, Job};
#[path = "../../ss14-motion-rules/src/port.rs"]
pub mod motion_rules;
pub mod net;
pub mod rounds;
pub use rounds::{Phase, RoundCommand, RoundSnapshot};
pub use station_sim::actions::{ActionSnapshot, WorkTarget};
pub use station_sim::interaction::{Command, InventorySnapshot, SLOTS, Verb, VerbKind};
// Authoritative simulation boundary. Rendering consumes snapshots only.
// The net module provides a separate TCP host and per-client snapshots.
use station_sim::Simulation;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct Snapshot {
    pub tick: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
}
pub struct Server {
    simulation: Simulation,
    seed: i64,
    rounds: Option<rounds::Rounds>,
    crew: crew::Roster,
}
impl Server {
    pub fn new(seed: i64) -> Self {
        Self::with_content(seed, Content::builtin())
    }
    pub fn with_content(seed: i64, content: Arc<Content>) -> Self {
        Self {
            simulation: Simulation::with_content(seed, content),
            seed,
            rounds: None,
            crew: crew::Roster::default(),
        }
    }
    pub fn with_rounds(seed: i64) -> Self {
        Self::rounds_content(seed, Content::builtin())
    }
    pub fn rounds_content(seed: i64, content: Arc<Content>) -> Self {
        let mut server = Self::with_content(seed, content);
        server.rounds = Some(rounds::Rounds::new());
        server.crew.join(0);
        server
    }
    pub fn round(&self) -> Option<RoundSnapshot> {
        self.rounds.as_ref().map(|r| r.snapshot)
    }
    fn playing(&self) -> bool {
        self.round().is_none_or(|r| r.phase == Phase::Playing)
    }
    fn epoch(&self) -> u64 {
        self.round().map_or(0, |r| r.epoch)
    }
    pub fn round_command(&mut self, epoch: u64, command: RoundCommand) {
        if epoch == self.epoch()
            && let Some(r) = &mut self.rounds
        {
            if let RoundCommand::Job(job) = command {
                if r.snapshot.phase == Phase::Lobby && self.crew.choose(0, job) {
                    r.command(RoundCommand::Ready(false));
                }
            } else {
                r.command(command);
                if matches!(command, RoundCommand::Lobby) && r.snapshot.phase == Phase::Lobby {
                    self.crew.reset();
                }
            }
        }
    }
    pub fn tick(&mut self, movement: [f32; 2]) -> Snapshot {
        if let Some(r) = &mut self.rounds {
            r.snapshot.world_changes = self.simulation.world_revision();
            if r.tick() {
                self.simulation = Simulation::with_content(self.seed, self.simulation.content());
                self.simulation.prepare_departments();
                self.simulation.outfit_actor(0, self.crew.job(0), false);
            }
        }
        if !self.playing() {
            return self.snapshot();
        }
        // Input validation belongs to authority, even for the in-process prototype.
        let movement = movement.map(|v| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        });
        self.simulation.step(movement);
        self.snapshot()
    }
    pub fn open_floor(&mut self) {
        if self.playing() {
            self.simulation.open_floor();
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            tick: self.simulation.tick,
            position: self.simulation.position(),
            velocity: self.simulation.velocity(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authority_rejects_nonfinite_movement() {
        let mut s = Server::new(7);
        let snap = s.tick([f32::NAN, f32::INFINITY]);
        assert_eq!(snap.tick, 1);
        assert_eq!(snap.position[0], 0.5);
        assert_eq!(snap.position[1], 0.5);
        assert!(snap.position.iter().all(|v| v.is_finite()));
    }
}

use station_sim::{Cell, DT};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Copy)]
pub struct Input {
    pub references: [Option<EntityId>; 2],
    pub sequence: u32,
    pub epoch: u64,
    pub round_command: Option<RoundCommand>,
    pub movement: [f32; 2],
    pub open_floor: bool,
    pub jump: bool,
    pub toggle_gravity: bool,
    pub reset: bool,
    pub command: Option<Command>,
    pub pickup: bool,
    pub drop_item: bool,
    pub throw_item: bool,
}
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Frame {
    pub floors: Vec<station_sim::floor::FloorTile>,
    pub barriers: Vec<station_sim::barrier::Barrier>,
    pub appearances: Vec<station_sim::appearance::ActorAppearance>,
    pub lamps: Vec<station_sim::content::Lamp>,
    pub power: station_sim::power::PowerSnapshot,
    pub rooms: Arc<station_sim::rooms::RoomsSnapshot>,
    pub character: station_sim::content::Character,
    pub entities: Vec<EntitySnapshot>,
    pub crew: Option<CrewSnapshot>,
    pub doors: Vec<station_sim::crew::DoorSnapshot>,
    pub acknowledged: u32,
    #[serde(skip)]
    pub rtt_ms: f32,
    #[serde(skip)]
    pub received_at: Option<Instant>,
    pub peers: Vec<(u64, [f32; 3])>,
    pub local_id: u64,
    pub host: bool,
    pub connected: bool,
    pub round: Option<RoundSnapshot>,
    pub action: Option<ActionSnapshot>,
    pub world_revision: u64,
    pub storage: station_sim::storage::StorageSnapshot,
    pub player: Snapshot,
    pub cells: Arc<Vec<Cell>>,
    pub items: Vec<station_sim::ItemSnapshot>,
    pub inventory: InventorySnapshot,
    pub verbs: Vec<Vec<Verb>>,
}
type QueuedCommand = (
    u64,
    Option<Command>,
    Option<RoundCommand>,
    [Option<EntityId>; 2],
);
struct Mailbox {
    input: Input,
    frame: Frame,
    stop: bool,
    commands: std::collections::VecDeque<QueuedCommand>,
}
/// Bounded latest-state mailbox: a slow renderer cannot grow an input/snapshot queue.
pub struct Session {
    mailbox: Arc<Mutex<Mailbox>>,
    worker: Option<JoinHandle<()>>,
}
impl Session {
    pub fn start(seed: i64) -> Self {
        Self::launch(Server::new(seed))
    }
    pub fn start_rounds(seed: i64) -> Self {
        Self::launch(Server::with_rounds(seed))
    }
    pub fn start_content(seed: i64, content: Arc<Content>) -> Self {
        Self::launch(Server::rounds_content(seed, content))
    }
    fn launch(server: Server) -> Self {
        let seed = server.seed;
        let frame = Frame {
            floors: server.simulation.floors(),
            barriers: server.simulation.content().map.effective_barriers(),
            appearances: server.simulation.actor_appearances(),
            lamps: server.simulation.powered_lamps(),
            power: server.simulation.power_snapshot(),
            rooms: server.simulation.rooms(),
            character: server.simulation.content().character.clone(),
            entities: server.simulation.entities(),
            crew: server.round().map(|_| server.crew.snapshot(0)),
            doors: server.simulation.door_snapshots(),
            acknowledged: 0,
            rtt_ms: 0.,
            received_at: None,
            peers: vec![],
            local_id: 0,
            host: true,
            connected: true,
            round: server.round(),
            action: server.simulation.action(),
            world_revision: server.simulation.world_revision(),
            storage: server.simulation.storage(),
            player: server.snapshot(),
            items: server.simulation.items(),
            inventory: server.simulation.inventory(),
            verbs: (0..server.simulation.items().len())
                .map(|id| server.simulation.verbs(id))
                .collect(),
            cells: Arc::new(server.simulation.world().cells().collect()),
        };
        let mailbox = Arc::new(Mutex::new(Mailbox {
            input: Input::default(),
            frame,
            stop: false,
            commands: std::collections::VecDeque::new(),
        }));
        let shared = mailbox.clone();
        let worker = thread::Builder::new()
            .name("station-server".into())
            .spawn(move || {
                let mut server = server;
                let interval = Duration::from_secs_f32(DT);
                let mut deadline = Instant::now();
                loop {
                    let (input, commands) = {
                        let mut m = shared.lock().unwrap();
                        if m.stop {
                            break;
                        }
                        let input = m.input;
                        m.input = Input {
                            epoch: input.epoch,
                            movement: input.movement,
                            ..Default::default()
                        };
                        (input, m.commands.drain(..).collect::<Vec<_>>())
                    };
                    for (epoch, command, round_command, references) in commands {
                        if epoch != server.epoch() {
                            continue;
                        }
                        if let Some(command) = round_command {
                            server.round_command(epoch, command);
                        }
                        if epoch == server.epoch()
                            && server.playing()
                            && let Some(command) = command
                            && references
                                == command_references(
                                    command,
                                    &server.simulation.items(),
                                    &server.simulation.door_snapshots(),
                                )
                        {
                            server.simulation.command(command);
                        }
                    }
                    let accepted = input.epoch == server.epoch() && server.playing();
                    if accepted {
                        if input.reset && server.rounds.is_none() {
                            server = Server::with_content(seed, server.simulation.content());
                        }
                        if input.open_floor {
                            server.open_floor();
                        }
                        if input.jump {
                            server.simulation.impulse([0., 0., 0.3]);
                        }
                        if input.toggle_gravity {
                            server.simulation.gravity = !server.simulation.gravity;
                        }
                        if input.pickup {
                            server.simulation.pickup();
                        }
                        if input.drop_item {
                            server.simulation.drop_item();
                        }
                        if input.throw_item {
                            server.simulation.throw_item();
                        }
                    }
                    let player = server.tick(if accepted { input.movement } else { [0.; 2] });
                    {
                        let mut m = shared.lock().unwrap();
                        if m.frame.round.map(|r| r.epoch) != server.round().map(|r| r.epoch) {
                            m.input = Input::default();
                            m.commands.clear();
                            m.frame.cells = Arc::new(server.simulation.world().cells().collect());
                        }
                        m.frame.floors = server.simulation.floors();
                        m.frame.appearances = server.simulation.actor_appearances();
                        m.frame.entities = server.simulation.entities();
                        m.frame.crew = server.round().map(|_| server.crew.snapshot(0));
                        m.frame.doors = server.simulation.door_snapshots();
                        m.frame.round = server.round();
                        m.frame.action = server.simulation.action();
                        m.frame.player = player;
                        m.frame.items = server.simulation.items();
                        m.frame.storage = server.simulation.storage();
                        m.frame.inventory = server.simulation.inventory();
                        m.frame.verbs = (0..m.frame.items.len())
                            .map(|id| server.simulation.verbs(id))
                            .collect();
                        if input.reset
                            || m.frame.world_revision != server.simulation.world_revision()
                        {
                            m.frame.world_revision = server.simulation.world_revision();
                            m.frame.cells = Arc::new(server.simulation.world().cells().collect());
                        }
                    }
                    deadline += interval;
                    let now = Instant::now();
                    if deadline > now {
                        thread::sleep(deadline - now);
                    } else {
                        deadline = now;
                    }
                }
            })
            .expect("start station server");
        Self {
            mailbox,
            worker: Some(worker),
        }
    }
    pub fn submit(&self, mut input: Input) {
        let mut m = self.mailbox.lock().unwrap();
        if input.epoch != m.frame.round.map_or(0, |r| r.epoch) {
            return;
        }
        if let Some(command) = input.command
            && input.references == [None; 2]
        {
            input.references = m.frame.references(command);
        }
        if (input.command.is_some() || input.round_command.is_some()) && m.commands.len() < 32 {
            m.commands.push_back((
                input.epoch,
                input.command,
                input.round_command,
                input.references,
            ));
        }
        m.input.epoch = input.epoch;
        m.input.movement = input.movement;
        m.input.open_floor |= input.open_floor;
        m.input.jump |= input.jump;
        m.input.toggle_gravity |= input.toggle_gravity;
        m.input.reset |= input.reset;
        m.input.pickup |= input.pickup;
        m.input.drop_item |= input.drop_item;
        m.input.throw_item |= input.throw_item;
    }
    pub fn frame(&self) -> Frame {
        self.mailbox.lock().unwrap().frame.clone()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.mailbox.lock().unwrap().stop = true;
        if let Some(worker) = self.worker.take() {
            worker.join().expect("server thread panicked");
        }
    }
}
#[cfg(test)]
mod session_tests {
    use super::*;
    fn wait(session: &Session, condition: impl Fn(&Frame) -> bool) -> Frame {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let frame = session.frame();
            if condition(&frame) {
                return frame;
            }
            assert!(Instant::now() < deadline, "server progress timeout");
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn commands_pick_drop_throw_and_publish_items() {
        let session = Session::start(7);
        let before = wait(&session, |f| f.player.tick > 90);
        assert_eq!(before.items.len(), 4);
        session.submit(Input {
            pickup: true,
            ..Default::default()
        });
        let held = wait(&session, |f| f.items[0].held);
        assert!(!before.items[0].held);
        session.submit(Input {
            drop_item: true,
            ..Default::default()
        });
        wait(&session, |f| {
            !f.items[0].held && f.player.tick > held.player.tick
        });
        session.submit(Input {
            pickup: true,
            ..Default::default()
        });
        let held = wait(&session, |f| f.items[0].held);
        session.submit(Input {
            throw_item: true,
            ..Default::default()
        });
        wait(&session, |f| {
            !f.items[0].held && f.items[0].position[0] > held.items[0].position[0] + 0.1
        });
    }
    #[test]
    fn server_ticks_without_renderer_and_replicates_destruction() {
        let session = Session::start(7);
        let upper = wait(&session, |f| f.player.tick > 90);
        assert!((upper.player.position[2] - 1.3).abs() < 0.05);
        session.submit(Input {
            open_floor: true,
            ..Default::default()
        });
        let lower = wait(&session, |f| f.player.position[2] < -2.65);
        assert!(!lower.cells.contains(&[0, 0, 0]));
        assert!(upper.cells.contains(&[0, 0, 0]));
        wait(&session, |f| f.items[0].position[2] < -2.6);
        assert!(upper.items[0].position[2] > 1.);
        assert!(
            upper
                .items
                .iter()
                .all(|i| i.rotation.iter().all(|v| v.is_finite()))
        );
        assert!(lower.cells.contains(&[0, 0, -4]));
        session.submit(Input {
            reset: true,
            ..Default::default()
        });
        wait(&session, |f| f.cells.contains(&[0, 0, 0]));
        drop(session);
    }
}

#[cfg(test)]
mod inventory_session_tests {
    use super::*;
    #[test]
    fn ordered_commands_survive_render_updates() {
        let s = Session::start(7);
        let deadline = Instant::now() + Duration::from_secs(8);
        while s.frame().player.tick < 90 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        for command in [
            Command::Verb {
                target: 0,
                kind: VerbKind::Pickup,
            },
            Command::Slot(11),
            Command::Slot(11),
            Command::Swap,
        ] {
            s.submit(Input {
                command: Some(command),
                ..Default::default()
            });
            s.submit(Input::default());
        }
        loop {
            let f = s.frame();
            if f.inventory.active == 1 {
                assert_eq!(f.inventory.hands, [Some(0), None]);
                assert_eq!(f.inventory.slots[11], None);
                assert!(!f.items[0].visible);
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod round_tests {
    use super::*;
    fn start(s: &mut Server) {
        s.round_command(s.epoch(), RoundCommand::Ready(true));
        for _ in 0..300 {
            s.tick([0.; 2]);
        }
        assert_eq!(s.round().unwrap().phase, Phase::Playing);
    }
    #[test]
    fn two_rounds_restore_world_inventory_physics_and_actions() {
        let mut s = Server::with_rounds(7);
        for _ in 0..20 {
            s.tick([1.; 2]);
        }
        assert_eq!(s.snapshot().tick, 0);
        start(&mut s);
        for _ in 0..90 {
            s.tick([0.; 2]);
        }
        s.simulation.pickup();
        assert_eq!(s.simulation.inventory().hands[0], Some(0));
        s.simulation.command(Command::Slot(11));
        s.open_floor();
        s.simulation.gravity = false;
        let old_epoch = s.epoch();
        s.round_command(old_epoch, RoundCommand::End);
        let tick = s.snapshot().tick;
        for _ in 0..20 {
            s.tick([1.; 2]);
        }
        assert_eq!(s.snapshot().tick, tick);
        assert!(s.round().unwrap().world_changes > 0);
        s.round_command(s.epoch(), RoundCommand::Lobby);
        s.round_command(old_epoch, RoundCommand::Ready(true));
        assert!(!s.round().unwrap().ready);
        start(&mut s);
        assert_eq!(s.round().unwrap().number, 2);
        assert!(s.simulation.world().solid([0, 0, 0]));
        assert_eq!(s.simulation.world_revision(), 0);
        assert_eq!(s.simulation.inventory().hands, [None; 2]);
        assert!(s.simulation.inventory().slots[10].is_some());
        let uniform = s.simulation.inventory().slots[7].expect("fresh round uniform");
        assert_eq!(s.simulation.items()[uniform].name, "uniform.assistant");
        assert!(
            s.simulation
                .inventory()
                .slots
                .iter()
                .enumerate()
                .all(|(i, v)| i == 10 || i == 7 || v.is_none())
        );
        assert!(s.simulation.storage().locked);
        assert!(!s.simulation.storage().open);
        assert!(s.simulation.storage().items.is_empty());
        assert!(s.simulation.action().is_none());
        assert!(s.simulation.gravity);
    }
    #[test]
    fn custom_content_survives_round_restart_but_entity_ids_do_not() {
        let mut data: serde_json::Value =
            serde_json::from_str(include_str!("../../content/station.json")).unwrap();
        data["prototypes"]["flashlight"]["half"] = 0.4.into();
        let content = Content::parse(&data.to_string()).unwrap();
        let mut s = Server::rounds_content(7, content);
        start(&mut s);
        let old = s.simulation.items()[0].entity.unwrap();
        s.open_floor();
        s.round_command(s.epoch(), RoundCommand::End);
        s.round_command(s.epoch(), RoundCommand::Lobby);
        start(&mut s);
        assert_eq!(s.simulation.items()[0].half, 0.4);
        assert!(s.simulation.world().solid([0, 0, 0]));
        assert!(!s.simulation.contains_entity(old));
        assert_ne!(old, s.simulation.items()[0].entity.unwrap());
    }
    #[test]
    fn stale_entity_reference_cannot_target_same_index_in_another_world() {
        let old = Simulation::new(7).items()[0].entity.unwrap();
        let session = Session::start(7);
        let deadline = Instant::now() + Duration::from_secs(4);
        while session.frame().player.tick < 90 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        session.submit(Input {
            command: Some(Command::Verb {
                target: 0,
                kind: VerbKind::Pickup,
            }),
            references: [Some(old), None],
            ..Default::default()
        });
        thread::sleep(Duration::from_millis(80));
        assert_eq!(session.frame().inventory.hands, [None; 2]);
        session.submit(Input {
            command: Some(Command::Verb {
                target: 0,
                kind: VerbKind::Pickup,
            }),
            ..Default::default()
        });
        while session.frame().inventory.hands[0] != Some(0) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn readiness_cancel_restarts_full_countdown() {
        let mut s = Server::with_rounds(7);
        s.round_command(s.epoch(), RoundCommand::Ready(true));
        for _ in 0..299 {
            s.tick([0.; 2]);
        }
        s.round_command(s.epoch(), RoundCommand::Ready(false));
        for _ in 0..300 {
            s.tick([0.; 2]);
        }
        assert_eq!(s.round().unwrap().phase, Phase::Lobby);
        assert_eq!(s.round().unwrap().remaining, 300);
        start(&mut s);
    }
    #[test]
    fn stale_mailbox_input_is_rejected() {
        let session = Session::start_rounds(7);
        session.submit(Input {
            epoch: 0,
            round_command: Some(RoundCommand::Ready(true)),
            pickup: true,
            ..Default::default()
        });
        thread::sleep(Duration::from_millis(60));
        let f = session.frame();
        assert!(!f.round.unwrap().ready);
        assert_eq!(f.player.tick, 0);
        assert!(f.inventory.hands.iter().all(Option::is_none));
    }
}

impl Frame {
    pub fn references(&self, c: Command) -> [Option<EntityId>; 2] {
        command_references(c, &self.items, &self.doors)
    }
}
fn command_references(
    c: Command,
    items: &[station_sim::ItemSnapshot],
    doors: &[station_sim::crew::DoorSnapshot],
) -> [Option<EntityId>; 2] {
    let item = |id: usize| items.get(id).and_then(|i| i.entity);
    match c {
        Command::Transfer { item: id, .. } => [item(id), None],
        Command::Door(id) => [doors.iter().find(|d| d.id == id).map(|d| d.entity), None],
        Command::Verb { target, kind } => [
            item(target),
            match kind {
                VerbKind::UseOn(id) | VerbKind::Pry(id) => item(id),
                _ => None,
            },
        ],
        Command::Store {
            container,
            item: id,
        }
        | Command::Take {
            container,
            item: id,
        } => [item(container), item(id)],
        Command::Work { tool, target } => [
            item(tool),
            if let WorkTarget::Door(id) = target {
                doors.iter().find(|d| d.id == id).map(|d| d.entity)
            } else if target == WorkTarget::Lock {
                item(station_sim::storage::CRATE)
            } else {
                None
            },
        ],
        _ => [None; 2],
    }
}
