//! Server-owned role reservations. A failed selection preserves the previous seat.
pub use station_sim::crew::Job;
use std::collections::BTreeMap;
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CrewSnapshot {
    pub selected: Job,
    pub vacancies: [usize; 3],
    pub feedback: String,
}
#[derive(Default)]
pub(crate) struct Roster {
    jobs: BTreeMap<u64, Job>,
    feedback: BTreeMap<u64, String>,
}
impl Roster {
    pub fn join(&mut self, id: u64) {
        self.jobs.entry(id).or_default();
    }
    pub fn leave(&mut self, id: u64) {
        self.jobs.remove(&id);
        self.feedback.remove(&id);
    }
    pub fn job(&self, id: u64) -> Job {
        self.jobs.get(&id).copied().unwrap_or_default()
    }
    pub fn choose(&mut self, id: u64, job: Job) -> bool {
        if !self.jobs.contains_key(&id) {
            return false;
        }
        if self.job(id) == job {
            return true;
        }
        if self.snapshot(id).vacancies[job.index()] == 0 {
            self.feedback.insert(id, "job.full".into());
            return false;
        }
        self.jobs.insert(id, job);
        self.feedback.remove(&id);
        true
    }
    pub fn reset(&mut self) {
        for job in self.jobs.values_mut() {
            *job = Job::Assistant;
        }
        self.feedback.clear();
    }
    pub fn snapshot(&self, id: u64) -> CrewSnapshot {
        let mut vacancies = [1_usize, 1, 8];
        for job in self.jobs.values() {
            vacancies[job.index()] = vacancies[job.index()].saturating_sub(1);
        }
        CrewSnapshot {
            selected: self.job(id),
            vacancies,
            feedback: self.feedback.get(&id).cloned().unwrap_or_default(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seats_are_exclusive_failure_preserves_role_and_disconnect_releases() {
        let mut r = Roster::default();
        r.join(1);
        r.join(2);
        assert!(r.choose(1, Job::Engineer));
        assert!(r.choose(2, Job::Doctor));
        assert!(!r.choose(2, Job::Engineer));
        assert_eq!(r.job(2), Job::Doctor);
        r.leave(1);
        assert!(r.choose(2, Job::Engineer));
        r.reset();
        assert_eq!(r.job(2), Job::Assistant);
        assert_eq!(r.snapshot(2).vacancies, [1, 1, 7]);
    }
}
