//! Local host round policy; SS14 contributes only the two readiness guards.
#[path = "../../ss14-round-rules/src/port.rs"]
mod rules;
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Lobby,
    Playing,
    Results,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub enum RoundCommand {
    Job(crate::Job),
    Ready(bool),
    End,
    Lobby,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct RoundSnapshot {
    pub phase: Phase,
    pub epoch: u64,
    pub number: u64,
    pub ready: bool,
    pub elapsed: u64,
    pub remaining: u64,
    pub world_changes: u64,
}
pub(crate) struct Rounds {
    pub snapshot: RoundSnapshot,
    countdown: u64,
    duration: u64,
}
impl Rounds {
    pub fn new() -> Self {
        Self {
            snapshot: RoundSnapshot {
                phase: Phase::Lobby,
                epoch: 1,
                number: 0,
                ready: false,
                elapsed: 0,
                remaining: 300,
                world_changes: 0,
            },
            countdown: 300,
            duration: 36000,
        }
    }
    fn transition(&mut self, phase: Phase) {
        self.snapshot.phase = phase;
        self.snapshot.epoch += 1;
    }
    pub fn command(&mut self, command: RoundCommand) {
        match command {
            RoundCommand::Ready(ready)
                if rules::can_ready(true, true, self.snapshot.phase == Phase::Lobby).unwrap() =>
            {
                self.snapshot.ready = ready;
                if !ready {
                    self.snapshot.remaining = self.countdown;
                }
            }
            RoundCommand::End if self.snapshot.phase == Phase::Playing => {
                self.transition(Phase::Results)
            }
            RoundCommand::Lobby if self.snapshot.phase == Phase::Results => {
                self.transition(Phase::Lobby);
                self.snapshot.ready = false;
                self.snapshot.remaining = self.countdown;
                self.snapshot.elapsed = 0;
            }
            _ => {}
        }
    }
    /// True only when a new simulation must be created.
    pub fn tick(&mut self) -> bool {
        match self.snapshot.phase {
            Phase::Lobby if rules::can_spawn(true, self.snapshot.ready, true).unwrap() => {
                self.snapshot.remaining = self.snapshot.remaining.saturating_sub(1);
                if self.snapshot.remaining == 0 {
                    self.transition(Phase::Playing);
                    self.snapshot.number += 1;
                    self.snapshot.elapsed = 0;
                    self.snapshot.world_changes = 0;
                    self.snapshot.remaining = self.duration;
                    return true;
                }
            }
            Phase::Playing => {
                self.snapshot.elapsed += 1;
                self.snapshot.remaining = self.duration.saturating_sub(self.snapshot.elapsed);
                if self.snapshot.remaining == 0 {
                    self.transition(Phase::Results);
                }
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadline_ends_once_and_results_stay_frozen() {
        let mut r = Rounds::new();
        r.countdown = 1;
        r.snapshot.remaining = 1;
        r.duration = 2;
        r.command(RoundCommand::Ready(true));
        assert!(r.tick());
        r.tick();
        r.tick();
        assert_eq!(r.snapshot.phase, Phase::Results);
        let epoch = r.snapshot.epoch;
        for _ in 0..10 {
            r.tick();
            r.command(RoundCommand::End);
        }
        assert_eq!(r.snapshot.epoch, epoch);
        assert_eq!(r.snapshot.elapsed, 2);
    }
}
