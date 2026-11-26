//! Station TCP protocol v15. Bounded nonblocking peers; one authoritative simulation.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
};
const VERSION: u32 = 15;
const MAX_PACKET: usize = 4 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Packet<T> {
    version: u32,
    data: T,
}
struct Wire {
    socket: TcpStream,
    read: Vec<u8>,
    write: Vec<u8>,
    offset: usize,
}
impl Wire {
    fn new(socket: TcpStream) -> io::Result<Self> {
        socket.set_nonblocking(true)?;
        socket.set_nodelay(true)?;
        Ok(Self {
            socket,
            read: vec![],
            write: vec![],
            offset: 0,
        })
    }
    fn send<T: Serialize>(&mut self, data: &T) -> io::Result<()> {
        if self.offset != self.write.len() {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&Packet {
            version: VERSION,
            data,
        })?;
        if bytes.len() > MAX_PACKET {
            return Err(io::Error::other("packet too large"));
        }
        self.write = (bytes.len() as u32).to_be_bytes().to_vec();
        self.write.extend(bytes);
        self.offset = 0;
        self.flush()
    }
    fn flush(&mut self) -> io::Result<()> {
        while self.offset < self.write.len() {
            match self.socket.write(&self.write[self.offset..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => self.offset += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn receive<T: serde::de::DeserializeOwned>(&mut self) -> io::Result<Vec<T>> {
        self.flush()?;
        let mut buffer = [0; 8192];
        // Bound work per peer, even if the sender continuously floods the socket.
        for _ in 0..32 {
            match self.socket.read(&mut buffer) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => self.read.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
            if self.read.len() > MAX_PACKET + 4 {
                return Err(io::Error::other("receive limit"));
            }
        }
        let mut result = vec![];
        while self.read.len() >= 4 && result.len() < 32 {
            let size = u32::from_be_bytes(self.read[..4].try_into().unwrap()) as usize;
            if size > MAX_PACKET {
                return Err(io::Error::other("packet limit"));
            }
            if self.read.len() < 4 + size {
                break;
            }
            let packet: Packet<T> = serde_json::from_slice(&self.read[4..4 + size])?;
            if packet.version != VERSION {
                return Err(io::Error::other("protocol version"));
            }
            result.push(packet.data);
            self.read.drain(..4 + size);
        }
        Ok(result)
    }
}
struct Peer {
    acknowledged: u32,
    sent_world: Option<(u64, u64)>,
    wire: Wire,
    movement: [f32; 2],
    last: Instant,
    ready: bool,
}
pub struct Host {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Host {
    pub fn bind(address: &str, seed: i64) -> io::Result<Self> {
        Self::bind_content(address, seed, Content::builtin())
    }
    pub fn bind_content(address: &str, seed: i64, content: Arc<Content>) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || run(listener, seed, stopped, content));
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            w.join().expect("network server");
        }
    }
}
fn run(listener: TcpListener, seed: i64, stop: Arc<AtomicBool>, content: Arc<Content>) {
    let mut server = Server::rounds_content(seed, content.clone());
    server.crew.leave(0);
    let mut peers = BTreeMap::<u64, Peer>::new();
    let mut next = 1;
    let mut cells = Arc::new(server.simulation.world().cells().collect::<Vec<_>>());
    let mut revision = 0;
    while !stop.load(Ordering::Relaxed) {
        let begin = Instant::now();
        for _ in 0..8 {
            match listener.accept() {
                Ok((socket, _)) if peers.len() < 8 => {
                    if let Ok(wire) = Wire::new(socket) {
                        server.simulation.add_actor(next);
                        server.crew.join(next);
                        if server.playing() {
                            server.simulation.outfit_actor(next, Job::Assistant, true);
                        }
                        peers.insert(
                            next,
                            Peer {
                                acknowledged: 0,
                                sent_world: None,
                                wire,
                                movement: [0.; 2],
                                last: Instant::now(),
                                ready: false,
                            },
                        );
                        next += 1;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let host = peers.keys().next().copied();
        let epoch = server.epoch();
        let mut remove = vec![];
        for (&id, peer) in &mut peers {
            match peer.wire.receive::<Input>() {
                Err(_) => {
                    remove.push(id);
                    continue;
                }
                Ok(inputs) => {
                    for input in inputs {
                        peer.last = Instant::now();
                        if motion_rules::discard_input(
                            input.sequence as i64,
                            peer.acknowledged as i64,
                        )
                        .unwrap()
                        {
                            continue;
                        }
                        peer.acknowledged = input.sequence;
                        if input.epoch != server.epoch() {
                            continue;
                        }
                        if let Some(command) = input.round_command {
                            match command {
                                RoundCommand::Job(job)
                                    if server.round().unwrap().phase == Phase::Lobby =>
                                {
                                    if server.crew.choose(id, job) {
                                        peer.ready = false;
                                    }
                                }
                                RoundCommand::Ready(ready)
                                    if server.round().unwrap().phase == Phase::Lobby =>
                                {
                                    peer.ready = ready
                                }
                                RoundCommand::End | RoundCommand::Lobby if host == Some(id) => {
                                    server.round_command(input.epoch, command)
                                }
                                _ => {}
                            }
                        }
                        if !server.playing() || input.epoch != server.epoch() {
                            continue;
                        }
                        peer.movement = input
                            .movement
                            .map(|v| if v.is_finite() { v.clamp(-1., 1.) } else { 0. });
                        server.simulation.select_actor(id);
                        if input.jump {
                            server.simulation.impulse([0., 0., 0.3]);
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
                        if let Some(command) = input.command
                            && input.references
                                == command_references(
                                    command,
                                    &server.simulation.items(),
                                    &server.simulation.door_snapshots(),
                                )
                        {
                            server.simulation.command(command);
                        }
                        // Debug resets, floor edits and gravity toggles are not network capabilities.
                    }
                }
            }
            if peer.last.elapsed() > Duration::from_secs(10) {
                remove.push(id);
            }
            if peer.last.elapsed() > Duration::from_millis(250) {
                peer.movement = [0.; 2];
            }
        }
        for id in remove {
            peers.remove(&id);
            server.crew.leave(id);
            server.simulation.remove_actor(id);
        }
        server.simulation.select_actor(0);
        if server.round().unwrap().phase == Phase::Lobby {
            let ready = !peers.is_empty() && peers.values().all(|p| p.ready);
            server.round_command(server.epoch(), RoundCommand::Ready(ready));
        }
        let rounds = server.rounds.as_mut().unwrap();
        rounds.snapshot.world_changes = server.simulation.world_revision();
        if rounds.tick() {
            server.simulation = Simulation::with_content(seed, content.clone());
            server.simulation.prepare_departments();
            for &id in peers.keys() {
                server.simulation.add_actor(id);
                server
                    .simulation
                    .outfit_actor(id, server.crew.job(id), false);
            }
        }
        if server.epoch() != epoch {
            if server.round().unwrap().phase == Phase::Lobby {
                server.crew.reset();
            }
            for peer in peers.values_mut() {
                peer.movement = [0.; 2];
                peer.ready = false;
            }
            cells = Arc::new(server.simulation.world().cells().collect());
        }
        if server.playing() {
            let inputs: Vec<_> = peers.iter().map(|(&id, p)| (id, p.movement)).collect();
            server.simulation.step_actors(&inputs);
        }
        if revision != server.simulation.world_revision() {
            revision = server.simulation.world_revision();
            cells = Arc::new(server.simulation.world().cells().collect());
        }
        let positions: Vec<_> = server
            .simulation
            .actor_positions()
            .into_iter()
            .filter(|(id, _)| *id != 0)
            .collect();
        let host = peers.keys().next().copied();
        for (&id, peer) in &mut peers {
            if peer.wire.offset != peer.wire.write.len() {
                continue;
            }
            let world_key = (server.epoch(), revision);
            server.simulation.select_actor(id);
            let mut round = server.round();
            if let Some(r) = &mut round {
                r.ready = peer.ready;
            }
            let frame = Frame {
                floors: if peer.sent_world == Some(world_key) {
                    vec![]
                } else {
                    server.simulation.floors()
                },
                barriers: server.simulation.content().map.effective_barriers(),
                appearances: server.simulation.actor_appearances(),
                lamps: server.simulation.powered_lamps(),
                power: server.simulation.power_snapshot(),
                rooms: if peer.sent_world == Some(world_key) {
                    Default::default()
                } else {
                    server.simulation.rooms()
                },
                character: server.simulation.content().character.clone(),
                entities: server.simulation.entities(),
                crew: Some(server.crew.snapshot(id)),
                doors: server.simulation.door_snapshots(),
                acknowledged: peer.acknowledged,
                rtt_ms: 0.,
                received_at: None,
                peers: positions.clone(),
                local_id: id,
                host: host == Some(id),
                connected: true,
                round,
                action: server.simulation.action(),
                world_revision: revision,
                storage: server.simulation.storage(),
                player: server.snapshot(),
                cells: if peer.sent_world == Some(world_key) {
                    Arc::new(vec![])
                } else {
                    cells.clone()
                },
                items: server.simulation.items(),
                inventory: server.simulation.inventory(),
                verbs: (0..server.simulation.items().len())
                    .map(|id| server.simulation.verbs(id))
                    .collect(),
            };
            peer.sent_world = Some(world_key);
            if peer.wire.send(&frame).is_err() {
                let _ = peer.wire.socket.shutdown(std::net::Shutdown::Both);
            }
        }
        server.simulation.select_actor(0);
        thread::sleep(Duration::from_secs_f32(DT).saturating_sub(begin.elapsed()));
    }
}

pub enum ClientSession {
    Local(Session),
    Remote(Remote),
}
impl ClientSession {
    pub fn start(seed: i64) -> Self {
        Self::Local(Session::start(seed))
    }
    pub fn start_rounds(seed: i64) -> Self {
        Self::Local(Session::start_rounds(seed))
    }
    pub fn start_content(seed: i64, content: Arc<Content>) -> Self {
        Self::Local(Session::start_content(seed, content))
    }
    pub fn connect(address: &str) -> io::Result<Self> {
        Ok(Self::Remote(Remote::connect(address)?))
    }
    pub fn submit(&self, mut input: Input) {
        if let Some(command) = input.command
            && input.references == [None; 2]
        {
            input.references = self.frame().references(command);
        }
        match self {
            Self::Local(s) => s.submit(input),
            Self::Remote(s) => {
                let _ = s.sender.try_send(input);
            }
        }
    }
    pub fn frame(&self) -> Frame {
        match self {
            Self::Local(s) => s.frame(),
            Self::Remote(s) => s.frame.lock().unwrap().clone(),
        }
    }
}
fn restore_map(incoming: &mut Frame, previous: &Frame) -> io::Result<()> {
    if incoming.cells.is_empty() {
        if incoming.world_revision != previous.world_revision
            || incoming.round.map(|r| r.epoch) != previous.round.map(|r| r.epoch)
        {
            return Err(io::Error::other("missing map for new revision"));
        }
        incoming.cells = previous.cells.clone();
        incoming.floors = previous.floors.clone();
        incoming.rooms = previous.rooms.clone();
    }
    Ok(())
}
pub struct Remote {
    frame: Arc<Mutex<Frame>>,
    sender: std::sync::mpsc::SyncSender<Input>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Remote {
    fn connect(address: &str) -> io::Result<Self> {
        use std::net::ToSocketAddrs;
        let address = address
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::other("address"))?;
        let mut wire = Wire::new(TcpStream::connect_timeout(
            &address,
            Duration::from_secs(5),
        )?)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut initial = loop {
            let mut latest: Option<Frame> = None;
            for mut frame in wire.receive::<Frame>()? {
                if frame.cells.is_empty() {
                    let prior = latest
                        .as_ref()
                        .ok_or_else(|| io::Error::other("missing initial map"))?;
                    restore_map(&mut frame, prior)?;
                }
                latest = Some(frame);
            }
            if let Some(frame) = latest {
                break frame;
            }
            if Instant::now() > deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            thread::sleep(Duration::from_millis(5));
        };
        initial.received_at = Some(Instant::now());
        let frame = Arc::new(Mutex::new(initial));
        let shared = frame.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Input>(64);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let mut sequence = 0_u32;
            let mut pending = BTreeMap::<u32, Instant>::new();
            let mut rtt_ms = 0.;
            let mut movement = Input::default();
            let mut heartbeat = Instant::now();
            let mut last_frame = Instant::now();
            while !stopped.load(Ordering::Relaxed) {
                let result = (|| -> io::Result<()> {
                    if wire.offset == wire.write.len() {
                        if let Ok(mut input) = receiver.try_recv() {
                            movement = Input {
                                epoch: input.epoch,
                                movement: input.movement,
                                ..Default::default()
                            };
                            sequence = sequence
                                .checked_add(1)
                                .ok_or_else(|| io::Error::other("input sequence exhausted"))?;
                            input.sequence = sequence;
                            pending.insert(sequence, Instant::now());
                            wire.send(&input)?;
                            heartbeat = Instant::now();
                        } else if heartbeat.elapsed() > Duration::from_millis(100) {
                            sequence = sequence
                                .checked_add(1)
                                .ok_or_else(|| io::Error::other("input sequence exhausted"))?;
                            movement.sequence = sequence;
                            pending.insert(sequence, Instant::now());
                            wire.send(&movement)?;
                            heartbeat = Instant::now();
                        }
                    }
                    while pending.len() > 256 {
                        pending.pop_first();
                    }
                    for mut incoming in wire.receive::<Frame>()? {
                        if let Some(sent) = pending.get(&incoming.acknowledged) {
                            let sample = sent.elapsed().as_secs_f32() * 1000.;
                            rtt_ms = if rtt_ms == 0. {
                                sample
                            } else {
                                rtt_ms * 0.8 + sample * 0.2
                            };
                        }
                        pending.retain(|&seq, _| {
                            !motion_rules::discard_input(seq as i64, incoming.acknowledged as i64)
                                .unwrap()
                        });
                        incoming.rtt_ms = rtt_ms;
                        incoming.received_at = Some(Instant::now());
                        let mut previous = shared.lock().unwrap();
                        restore_map(&mut incoming, &previous)?;
                        *previous = incoming;
                        last_frame = Instant::now();
                    }
                    if last_frame.elapsed() > Duration::from_secs(5) {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    Ok(())
                })();
                if result.is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(2));
            }
            shared.lock().unwrap().connected = false;
        });
        Ok(Self {
            frame,
            sender,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            w.join().expect("network client");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[track_caller]
    fn wait(client: &ClientSession, predicate: impl Fn(&Frame) -> bool) -> Frame {
        let deadline = Instant::now() + Duration::from_secs(9);
        loop {
            let f = client.frame();
            assert!(f.connected, "connection lost");
            if predicate(&f) {
                return f;
            }
            assert!(
                Instant::now() < deadline,
                "network timeout: pos={:?}, tick={}, msg={}, revision={}",
                f.player.position,
                f.player.tick,
                f.inventory.message,
                f.world_revision
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn send(c: &ClientSession, mut input: Input) {
        input.epoch = c.frame().round.unwrap().epoch;
        c.submit(input);
    }
    #[test]
    fn rooms_replicate_merge_vertical_connection_and_space_breach() {
        let mut content =
            (*Content::parse(include_str!("../../content/rooms-lab.json")).unwrap()).clone();
        content
            .kits
            .get_mut("job.engineer")
            .unwrap()
            .push(station_sim::content::KitItem {
                prototype: "crowbar".into(),
                slot: 12,
            });
        let host = Host::bind_content("127.0.0.1:0", 7, Arc::new(content)).unwrap();
        let a = ClientSession::connect(&host.address.to_string()).unwrap();
        let b = ClientSession::connect(&host.address.to_string()).unwrap();
        for (client, job) in [(&a, Job::Engineer), (&b, Job::Doctor)] {
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Job(job)),
                    ..Default::default()
                },
            );
            wait(client, |f| {
                f.crew.as_ref().is_some_and(|c| c.selected == job)
            });
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Ready(true)),
                    ..Default::default()
                },
            );
        }
        let first = wait(&a, |f| f.player.tick > 90);
        assert_ne!(
            first.rooms.at([-2, 0, 0]).unwrap().id,
            first.rooms.at([2, 0, 0]).unwrap().id
        );
        let command = |c| {
            send(
                &a,
                Input {
                    command: Some(c),
                    ..Default::default()
                },
            )
        };
        command(Command::Door(0));
        for client in [&a, &b] {
            let opened = wait(client, |f| f.doors[0].open);
            assert_eq!(
                opened.rooms.at([-2, 0, 0]).unwrap().id,
                opened.rooms.at([2, 0, 0]).unwrap().id
            );
            assert!(!opened.rooms.at([2, 0, 0]).unwrap().space);
        }
        command(Command::Slot(11));
        let equipped = wait(&a, |f| f.inventory.hands[0].is_some());
        let tool = equipped.inventory.hands[0].unwrap();
        command(Command::Work {
            target: WorkTarget::Floor([-1, 0, 0]),
            tool,
        });
        let lower = wait(&a, |f| {
            f.player.position[2] < -2.6
                && f.player.position[2] > -2.9
                && f.player.velocity[2].abs() < 0.05
        });
        assert_eq!(
            lower.rooms.at([-1, 0, 0]).unwrap().id,
            lower.rooms.at([-1, 0, -4]).unwrap().id
        );
        assert!(!lower.rooms.at([-1, 0, 0]).unwrap().space);
        // Falling can pull the held physics tool out of the hand. Take a secured spare.
        wait(&a, |f| {
            f.player.tick >= lower.player.tick + 30 && f.player.velocity[2].abs() < 0.05
        });
        let spare = lower.inventory.slots[12].unwrap();
        command(Command::Slot(12));
        wait(&a, |f| f.inventory.hands[0] == Some(spare));
        let tool = spare;
        command(Command::Work {
            target: WorkTarget::Floor([-1, 0, -4]),
            tool,
        });
        for client in [&a, &b] {
            let exposed = wait(client, |f| f.rooms.at([2, 0, 0]).is_some_and(|r| r.space));
            assert!(
                exposed
                    .rooms
                    .at([2, 0, 0])
                    .unwrap()
                    .breaches
                    .contains(&[-1, 0, -4])
            );
            let cached = wait(client, |f| f.player.tick > exposed.player.tick + 5);
            assert!(
                Arc::ptr_eq(&cached.rooms, &exposed.rooms),
                "unchanged topology must reuse received snapshot"
            );
        }
        let observer = ClientSession::connect(&host.address.to_string()).unwrap();
        assert!(observer.frame().rooms.at([2, 0, 0]).unwrap().space);
    }
    #[test]
    fn power_cut_replicates_dark_room_manual_door_and_repair() {
        use station_sim::{
            floor::{FloorKind, FloorTile},
            power::{PowerDef, PowerNode},
        };
        let mut c = (*Content::test_station()).clone();
        c.map.crate_round_position = [6., -4., 1.4];
        c.map.power = Some(PowerDef {
            cables: vec![[-6, 2, 0], [-5, 2, 0], [-4, 2, 0], [5, 2, 0]],
            sources: vec![
                PowerNode {
                    cell: [-6, 2, 0],
                    enabled: true,
                },
                PowerNode {
                    cell: [5, 2, 0],
                    enabled: true,
                },
            ],
            controllers: vec![
                PowerNode {
                    cell: [-4, 2, 0],
                    enabled: true,
                },
                PowerNode {
                    cell: [5, 2, 0],
                    enabled: true,
                },
            ],
        });
        c.map.floors.push(FloorTile {
            cell: [-5, 2, 0],
            kind: FloorKind::Plating,
            variant: None,
        });
        let host = Host::bind_content("127.0.0.1:0", 7, Arc::new(c)).unwrap();
        let a = ClientSession::connect(&host.address.to_string()).unwrap();
        let b = ClientSession::connect(&host.address.to_string()).unwrap();
        for (client, job) in [(&a, Job::Engineer), (&b, Job::Doctor)] {
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Job(job)),
                    ..Default::default()
                },
            );
            wait(client, |f| {
                f.crew.as_ref().is_some_and(|c| c.selected == job)
            });
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Ready(true)),
                    ..Default::default()
                },
            );
        }
        wait(&a, |f| f.player.tick > 90);
        let command = |c| {
            send(
                &a,
                Input {
                    command: Some(c),
                    ..Default::default()
                },
            )
        };
        command(Command::Slot(8));
        let held = wait(&a, |f| f.inventory.hands[0].is_some());
        let tool = held.inventory.hands[0].unwrap();
        command(Command::Work {
            target: WorkTarget::Cable([-5, 2, 0]),
            tool,
        });
        for client in [&a, &b] {
            let dark = wait(client, |f| !f.doors[0].powered && f.power.revision == 1);
            assert_eq!(dark.lamps[1].energy, 0.);
            assert!(dark.lamps[2].energy > 0.);
            assert!(dark.doors[1].powered);
            assert!(dark.cells.contains(&[-5, 2, 0]));
        }
        command(Command::Slot(8));
        wait(&a, |f| f.inventory.hands[0].is_none());
        command(Command::Slot(11));
        let held = wait(&a, |f| f.inventory.hands[0].is_some());
        command(Command::Work {
            target: WorkTarget::Door(0),
            tool: held.inventory.hands[0].unwrap(),
        });
        for client in [&a, &b] {
            wait(client, |f| {
                f.doors[0].open && !f.cells.contains(&[-3, 2, 1])
            });
        }
        command(Command::Slot(11));
        wait(&a, |f| f.inventory.hands[0].is_none());
        command(Command::Slot(8));
        wait(&a, |f| f.inventory.hands[0] == Some(tool));
        command(Command::Work {
            target: WorkTarget::Cable([-5, 2, 0]),
            tool,
        });
        for client in [&a, &b] {
            let lit = wait(client, |f| f.power.revision == 2);
            assert!(lit.doors[0].powered && lit.lamps[1].energy > 0.);
        }
    }
    #[test]
    fn two_clients_lift_and_lay_one_surface_without_duplicating_material() {
        use station_sim::{ItemKind, floor::FloorKind};
        let mut content = (*Content::test_station()).clone();
        content
            .map
            .spawns
            .insert("job.engineer".into(), [-0.3, 0.5, 1.3]);
        content
            .map
            .spawns
            .insert("job.doctor".into(), [1.3, 0.5, 1.3]);
        content.map.crate_round_position = [6., -4., 1.4];
        content
            .prototypes
            .get_mut("tile.steel")
            .unwrap()
            .stack_count = 1;
        let doctor = content.kits.get_mut("job.doctor").unwrap();
        doctor.iter_mut().find(|i| i.slot == 11).unwrap().prototype = "crowbar".into();
        doctor.push(station_sim::content::KitItem {
            prototype: "tile.steel".into(),
            slot: 12,
        });
        let host = Host::bind_content("127.0.0.1:0", 7, Arc::new(content)).unwrap();
        let a = ClientSession::connect(&host.address.to_string()).unwrap();
        let b = ClientSession::connect(&host.address.to_string()).unwrap();
        for (client, job) in [(&a, Job::Engineer), (&b, Job::Doctor)] {
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Job(job)),
                    ..Default::default()
                },
            );
            wait(client, |f| {
                f.crew.as_ref().is_some_and(|c| c.selected == job)
            });
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Ready(true)),
                    ..Default::default()
                },
            );
        }
        let tile_at = |f: &Frame| {
            f.floors
                .iter()
                .find(|t| t.cell == [0, 0, 0])
                .map_or(FloorKind::Steel, |t| t.kind)
        };
        let mut stacks = vec![];
        for client in [&a, &b] {
            let f = wait(client, |f| f.player.tick > 90);
            stacks.push(f.inventory.slots[12].unwrap());
            send(
                client,
                Input {
                    command: Some(Command::Slot(11)),
                    ..Default::default()
                },
            );
            wait(client, |f| {
                f.inventory.hands[0].is_some_and(|id| f.items[id].kind == ItemKind::Crowbar)
            });
        }
        for client in [&a, &b] {
            let tool = client.frame().inventory.hands[0].unwrap();
            send(
                client,
                Input {
                    command: Some(Command::Work {
                        target: WorkTarget::LiftTile([0, 0, 0]),
                        tool,
                    }),
                    ..Default::default()
                },
            );
        }
        for client in [&a, &b] {
            wait(client, |f| {
                tile_at(f) == FloorKind::Plating && f.action.is_none()
            });
        }
        let lifted = a.frame();
        assert!(lifted.cells.contains(&[0, 0, 0]));
        assert_eq!(
            stacks
                .iter()
                .map(|&i| lifted.items[i].stack_count)
                .sum::<u16>(),
            3
        );
        for client in [&a, &b] {
            send(
                client,
                Input {
                    command: Some(Command::Hand(1)),
                    ..Default::default()
                },
            );
            wait(client, |f| f.inventory.active == 1);
            send(
                client,
                Input {
                    command: Some(Command::Slot(12)),
                    ..Default::default()
                },
            );
            wait(client, |f| {
                f.inventory.hands[1].is_some_and(|i| matches!(f.items[i].kind, ItemKind::Tile(_)))
            });
        }
        for client in [&a, &b] {
            let tool = client.frame().inventory.hands[1].unwrap();
            send(
                client,
                Input {
                    command: Some(Command::Work {
                        target: WorkTarget::LayTile([0, 0, 0]),
                        tool,
                    }),
                    ..Default::default()
                },
            );
        }
        for client in [&a, &b] {
            let done = wait(client, |f| {
                tile_at(f) == FloorKind::Steel && f.action.is_none()
            });
            assert_eq!(done.world_revision, lifted.world_revision + 1);
            assert_eq!(
                stacks
                    .iter()
                    .map(|&i| done.items[i].stack_count)
                    .sum::<u16>(),
                2
            );
            assert!(done.cells.contains(&[0, 0, 0]));
            assert!((done.player.position[2] - 1.3).abs() < 0.05);
        }
    }
    #[test]
    fn test_station_windows_replicate_and_door_opens_a_physical_route() {
        let mut c = (*Content::test_station()).clone();
        c.map
            .spawns
            .insert("job.engineer".into(), [-1.5, -7.5, 1.3]);
        c.map.spawns.insert("job.doctor".into(), [-5., -7.5, 1.3]);
        let host = Host::bind_content("127.0.0.1:0", 7, Arc::new(c)).unwrap();
        let a = ClientSession::connect(&host.address.to_string()).unwrap();
        let b = ClientSession::connect(&host.address.to_string()).unwrap();
        send(
            &a,
            Input {
                round_command: Some(RoundCommand::Job(Job::Engineer)),
                ..Default::default()
            },
        );
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::Job(Job::Doctor)),
                ..Default::default()
            },
        );
        wait(&a, |f| {
            f.crew.as_ref().is_some_and(|c| c.selected == Job::Engineer)
        });
        wait(&b, |f| {
            f.crew.as_ref().is_some_and(|c| c.selected == Job::Doctor)
        });
        for client in [&a, &b] {
            send(
                client,
                Input {
                    round_command: Some(RoundCommand::Ready(true)),
                    ..Default::default()
                },
            );
        }
        let first = wait(&a, |f| f.round.is_some_and(|r| r.phase == Phase::Playing));
        let other = wait(&b, |f| f.round.is_some_and(|r| r.phase == Phase::Playing));
        assert_eq!(first.barriers, other.barriers);
        assert_eq!(first.barriers.len(), 16);
        assert!(first.doors[3].glass);
        let glass = first
            .barriers
            .iter()
            .find(|b| b.cell == [3, -10, 1])
            .unwrap();
        assert!(glass.blocks_movement && !glass.blocks_sight && !glass.blocks_light);
        send(
            &a,
            Input {
                movement: [-1., 0.],
                ..Default::default()
            },
        );
        let stopped = wait(&a, |f| f.player.tick >= first.player.tick + 60);
        assert!(
            stopped.player.position[0] >= -1.72,
            "window collider missing"
        );
        send(
            &a,
            Input {
                movement: [0., -1.],
                ..Default::default()
            },
        );
        wait(&a, |f| f.player.position[1] < -9.4);
        send(&a, Input::default());
        wait(&a, |f| f.doors[2].reachable);
        send(
            &a,
            Input {
                command: Some(Command::Door(2)),
                ..Default::default()
            },
        );
        wait(&a, |f| f.doors[2].open && !f.cells.contains(&[-3, -10, 1]));
        wait(&b, |f| f.doors[2].open);
        send(
            &a,
            Input {
                movement: [-1., 0.],
                ..Default::default()
            },
        );
        let crossed = wait(&a, |f| f.player.position[0] < -3.4);
        send(&a, Input::default());
        wait(&b, |f| {
            f.peers
                .iter()
                .any(|(id, p)| *id == crossed.local_id && p[0] < -3.4)
        });
        assert!(crossed.cells.contains(&[-3, -8, 1]));
    }
    #[test]
    fn two_tcp_clients_round_movement_ownership_disconnect_and_rejoin() {
        let mut content = (*Content::builtin()).clone();
        let floor = station_sim::floor::FloorTile {
            cell: [2, 1, 0],
            kind: station_sim::floor::FloorKind::Damaged,
            variant: Some(2),
        };
        content.map.floors.push(floor);
        let host = Host::bind_content("127.0.0.1:0", 7, Arc::new(content)).unwrap();
        let address = host.address.to_string();
        let a = ClientSession::connect(&address).unwrap();
        let b = ClientSession::connect(&address).unwrap();
        wait(&a, |f| f.peers.len() == 2);
        assert_ne!(a.frame().local_id, b.frame().local_id);
        assert!(a.frame().host);
        assert!(!b.frame().host);
        send(
            &a,
            Input {
                round_command: Some(RoundCommand::Ready(true)),
                ..Default::default()
            },
        );
        thread::sleep(Duration::from_millis(100));
        assert_eq!(a.frame().round.unwrap().phase, Phase::Lobby);
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::Ready(true)),
                ..Default::default()
            },
        );
        let first = wait(&a, |f| f.player.tick > 90);
        let second = wait(&b, |f| f.player.tick > 90);
        assert_eq!(first.floors, vec![floor]);
        assert_eq!(second.floors, first.floors);
        send(
            &a,
            Input {
                pickup: true,
                ..Default::default()
            },
        );
        wait(&a, |f| f.inventory.hands[0] == Some(0));
        send(
            &b,
            Input {
                pickup: true,
                round_command: Some(RoundCommand::End),
                open_floor: true,
                toggle_gravity: true,
                ..Default::default()
            },
        );
        thread::sleep(Duration::from_millis(100));
        assert_ne!(b.frame().inventory.hands[0], Some(0));
        assert_eq!(b.frame().round.unwrap().phase, Phase::Playing);
        assert_eq!(b.frame().world_revision, 0);
        send(
            &b,
            Input {
                movement: [0., 1.],
                ..Default::default()
            },
        );
        wait(&b, |f| f.player.position[1] > 1.4);
        send(
            &b,
            Input {
                command: Some(Command::Verb {
                    target: 3,
                    kind: VerbKind::Pickup,
                }),
                ..Default::default()
            },
        );
        wait(&b, |f| f.inventory.hands[0] == Some(3));
        send(
            &b,
            Input {
                command: Some(Command::Work {
                    target: WorkTarget::Floor([2, 1, 0]),
                    tool: 3,
                }),
                ..Default::default()
            },
        );
        wait(&b, |f| {
            f.world_revision == 1 && !f.cells.contains(&[2, 1, 0]) && f.floors.is_empty()
        });
        wait(&a, |f| {
            f.world_revision == 1 && !f.cells.contains(&[2, 1, 0]) && f.floors.is_empty()
        });

        send(
            &b,
            Input {
                movement: [-1., 0.],
                ..Default::default()
            },
        );
        let moved = wait(&b, |f| f.player.position[0] < 1.8);
        let view = wait(&a, |f| {
            f.peers
                .iter()
                .any(|(id, p)| *id == moved.local_id && p[0] < 1.8)
        });
        assert!((view.player.position[0] - first.player.position[0]).abs() < 0.2);
        drop(a);
        wait(&b, |f| f.host && f.peers.len() == 1 && f.items[0].visible);
        let c = ClientSession::connect(&address).unwrap();
        assert_ne!(c.frame().local_id, first.local_id);
        assert!(c.frame().inventory.hands.iter().all(Option::is_none));
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::End),
                ..Default::default()
            },
        );
        let results = wait(&b, |f| f.round.unwrap().phase == Phase::Results);
        c.submit(Input {
            epoch: first.round.unwrap().epoch,
            movement: [1., 0.],
            pickup: true,
            ..Default::default()
        });
        thread::sleep(Duration::from_millis(100));
        assert_eq!(b.frame().player.tick, results.player.tick);
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::Lobby),
                ..Default::default()
            },
        );
        wait(&b, |f| f.round.unwrap().phase == Phase::Lobby);
        wait(&c, |f| f.round.unwrap().phase == Phase::Lobby);
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::Ready(true)),
                ..Default::default()
            },
        );
        send(
            &c,
            Input {
                round_command: Some(RoundCommand::Ready(true)),
                ..Default::default()
            },
        );
        let fresh = wait(&b, |f| f.round.unwrap().number == 2);
        assert!(fresh.cells.contains(&[0, 0, 0]));
        assert_eq!(fresh.world_revision, 0);
        assert_eq!(fresh.floors, vec![floor]);
        assert!(fresh.inventory.hands.iter().all(Option::is_none));
        assert!(fresh.storage.locked);
        drop(host);
        let deadline = Instant::now() + Duration::from_secs(2);
        while b.frame().connected {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn professions_capacity_access_late_join_and_next_round() {
        let host = Host::bind("127.0.0.1:0", 7).unwrap();
        let address = host.address.to_string();
        let a = ClientSession::connect(&address).unwrap();
        let b = ClientSession::connect(&address).unwrap();
        send(
            &a,
            Input {
                round_command: Some(RoundCommand::Job(Job::Engineer)),
                ..Default::default()
            },
        );
        wait(&a, |f| f.crew.as_ref().unwrap().selected == Job::Engineer);
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::Job(Job::Engineer)),
                ..Default::default()
            },
        );
        let denied = wait(&b, |f| f.crew.as_ref().unwrap().feedback == "job.full");
        assert_eq!(denied.crew.unwrap().selected, Job::Assistant);
        send(
            &b,
            Input {
                round_command: Some(RoundCommand::Job(Job::Doctor)),
                ..Default::default()
            },
        );
        wait(&b, |f| f.crew.as_ref().unwrap().selected == Job::Doctor);
        for c in [&a, &b] {
            send(
                c,
                Input {
                    round_command: Some(RoundCommand::Ready(true)),
                    ..Default::default()
                },
            );
        }
        let engineer = wait(&a, |f| f.player.tick > 90);
        let doctor = wait(&b, |f| f.player.tick > 90);
        assert!(engineer.player.position[0] < -3.);
        assert!(doctor.player.position[0] > 4.);
        assert_eq!(
            engineer.items[engineer.inventory.slots[11].unwrap()].name,
            "item.crowbar"
        );
        assert_eq!(
            doctor.items[doctor.inventory.slots[11].unwrap()].name,
            "item.medkit"
        );
        assert!(engineer.doors[0].allowed && !engineer.doors[1].allowed);
        assert!(doctor.doors[1].allowed && !doctor.doors[0].allowed);
        send(
            &a,
            Input {
                command: Some(Command::Door(0)),
                ..Default::default()
            },
        );
        for client in [&a, &b] {
            let moving = wait(client, |f| {
                f.doors[0].phase == station_sim::crew::DoorPhase::Opening
            });
            assert!(!moving.doors[0].open);
            assert!(moving.cells.contains(&[-3, 2, 1]));
            assert!(moving.doors[0].elapsed < station_sim::crew::DOOR_TICKS);
        }
        // A newly connected observer receives the ongoing phase, not a restarted animation.
        let observer = ClientSession::connect(&host.address.to_string()).unwrap();
        let current = observer.frame();
        assert_ne!(current.doors[0].phase, station_sim::crew::DoorPhase::Closed);
        drop(observer);
        wait(&b, |f| f.doors[0].open && !f.cells.contains(&[-3, 2, 1]));
        send(
            &b,
            Input {
                command: Some(Command::Door(1)),
                round_command: Some(RoundCommand::Job(Job::Engineer)),
                ..Default::default()
            },
        );
        let both = wait(&a, |f| f.doors.iter().all(|d| d.open));
        assert_eq!(both.world_revision, 2);
        assert_eq!(b.frame().crew.unwrap().selected, Job::Doctor);
        let late = ClientSession::connect(&address).unwrap();
        let arrived = late.frame();
        assert_eq!(arrived.crew.unwrap().selected, Job::Assistant);
        assert!(arrived.player.position[1] < -3.);
        assert!(arrived.inventory.slots[10].is_some());
        send(
            &a,
            Input {
                round_command: Some(RoundCommand::End),
                ..Default::default()
            },
        );
        wait(&a, |f| f.round.unwrap().phase == Phase::Results);
        send(
            &a,
            Input {
                round_command: Some(RoundCommand::Lobby),
                ..Default::default()
            },
        );
        for c in [&a, &b, &late] {
            let lobby = wait(c, |f| f.round.unwrap().phase == Phase::Lobby);
            assert_eq!(lobby.crew.as_ref().unwrap().selected, Job::Assistant);
            assert_eq!(&lobby.crew.as_ref().unwrap().vacancies[..2], &[1, 1]);
            send(
                c,
                Input {
                    round_command: Some(RoundCommand::Ready(true)),
                    ..Default::default()
                },
            );
        }
        let fresh = wait(&a, |f| f.round.unwrap().number == 2);
        assert!(fresh.doors.iter().all(|d| !d.open && !d.allowed));
        assert!(fresh.cells.contains(&[-3, 2, 1]));
        assert_eq!(fresh.inventory.slots[11], None);
        assert_eq!(
            fresh.items[fresh.inventory.slots[10].unwrap()].name,
            "item.id.assistant"
        );
    }
    #[test]
    fn malformed_or_oversized_peer_does_not_stop_host() {
        let host = Host::bind("127.0.0.1:0", 7).unwrap();
        for bytes in [u32::MAX.to_be_bytes().to_vec(), vec![0, 0, 0, 1, b'!']] {
            let mut socket = TcpStream::connect(host.address).unwrap();
            socket.write_all(&bytes).unwrap();
            thread::sleep(Duration::from_millis(40));
        }
        let client = ClientSession::connect(&host.address.to_string()).unwrap();
        assert!(client.frame().connected);
    }
}
#[cfg(test)]
mod delayed_transport_tests {
    use super::*;
    use std::collections::VecDeque;
    struct Proxy {
        address: SocketAddr,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }
    impl Proxy {
        fn start(upstream: SocketAddr, round_trip_ms: u64) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let worker = thread::spawn(move || {
                let mut client = loop {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok((stream, _)) = listener.accept() {
                        break stream;
                    }
                    thread::sleep(Duration::from_millis(1));
                };
                let mut server = TcpStream::connect(upstream).unwrap();
                client.set_nonblocking(true).unwrap();
                server.set_nonblocking(true).unwrap();
                let mut up = VecDeque::new();
                let mut down = VecDeque::new();
                let mut counter = 0;
                while !stopped.load(Ordering::Relaxed) {
                    let delay = Duration::from_millis(round_trip_ms / 2 + (counter % 3) * 3);
                    if pump(&mut client, &mut server, &mut up, delay).is_err()
                        || pump(&mut server, &mut client, &mut down, delay).is_err()
                    {
                        break;
                    }
                    counter += 1;
                    thread::sleep(Duration::from_millis(1));
                }
            });
            Self {
                address,
                stop,
                worker: Some(worker),
            }
        }
    }
    type Delayed = VecDeque<(Instant, Vec<u8>, usize)>;
    fn pump(
        from: &mut TcpStream,
        to: &mut TcpStream,
        queue: &mut Delayed,
        delay: Duration,
    ) -> io::Result<()> {
        let mut buffer = [0; 8192];
        for _ in 0..8 {
            match from.read(&mut buffer) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => queue.push_back((Instant::now() + delay, buffer[..n].to_vec(), 0)),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        if queue.len() > 128 {
            return Err(io::Error::other("proxy backlog"));
        }
        while let Some((due, bytes, offset)) = queue.front_mut() {
            if *due > Instant::now() {
                break;
            }
            match to.write(&bytes[*offset..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => *offset += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
            if *offset == bytes.len() {
                queue.pop_front();
            }
        }
        Ok(())
    }
    impl Drop for Proxy {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.worker.take().unwrap().join().unwrap();
        }
    }
    #[test]
    fn acknowledgements_measure_100_and_200_ms_rtt_and_connection_loss() {
        for delay in [100, 200] {
            let host = Host::bind("127.0.0.1:0", 7).unwrap();
            let proxy = Proxy::start(host.address, delay);
            let client = ClientSession::connect(&proxy.address.to_string()).unwrap();
            let initial = client.frame();
            assert!(!initial.cells.is_empty(), "fragmented initial map lost");
            client.submit(Input {
                epoch: initial.round.unwrap().epoch,
                round_command: Some(RoundCommand::Ready(true)),
                ..Default::default()
            });
            let deadline = Instant::now() + Duration::from_secs(4);
            let measured = loop {
                let frame = client.frame();
                assert!(frame.connected);
                if frame.acknowledged >= 3 && frame.rtt_ms > 0. {
                    break frame;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            };
            assert!(
                measured.rtt_ms >= delay as f32 * 0.8,
                "{} vs {delay}",
                measured.rtt_ms
            );
            assert!(measured.rtt_ms < delay as f32 + 250.);
            assert!(measured.round.unwrap().ready);
            println!("injected RTT {delay}ms, measured {:.1}ms", measured.rtt_ms);
            drop(proxy);
            while client.frame().connected {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
