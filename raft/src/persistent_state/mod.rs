// persistent state

use atomic_write_file::AtomicWriteFile;
use std::path::Path;

pub struct PersistentState {
    // These data must be stored on persistent storage
    current_term: u64,
    voted_for: Option<&'static str>,
}

impl PersistentState {
    pub fn new(path: &Path) -> PersistentState {
        // TODO : initialize with data from path
        PersistentState {
            current_term: 0,
            voted_for: None,
        }
    }

    pub fn current_term(&self) -> u64 {
        self.current_term
    }

    pub fn voted_for(&self) -> Option<&'static str> {
        self.voted_for
    }

    // Dummy implementation
    pub fn start_election(&mut self, self_id: &'static str) {
        self.current_term = self.current_term + 1;
        self.voted_for = Some(self_id);
    }

    // Dummy implementation.
    // return (current_term, ok)
    //   current_term : term number after update
    //   ok : true if the given term was not outdated
    pub fn update_term(&mut self, new_term: u64) -> (u64, bool) {
        if new_term < self.current_term {
            (self.current_term, false)
        } else {
            // Warning : this two updates must be committed simultaneously
            self.current_term = new_term;
            self.voted_for = None;
            (self.current_term, true)
        }
    }

    // Dummy implementation
    // return ok
    //   ok : true if candidate received a vote
    pub fn try_vote(&mut self, candidate: &'static str) -> bool {
        match &self.voted_for {
            None => {
                self.voted_for = Some(candidate);
                true
            }
            Some(recipient) => *recipient == candidate,
        }
    }
}
