use crate::raftchat_tonic::{Command, Entry};
use crate::wal::{Action, WAL};
use std::collections::HashMap;

pub trait StateMachine {
    fn new() -> Self;
    fn apply(&mut self, cmd: &Command);

    fn apply_entries(&mut self, entries: &[Entry]) {
        for entry in entries {
            if let Some(cmd) = &entry.command {
                self.apply(cmd);
            }
        }
    }
}

pub struct SMWrapper<S> {
    wal: WAL,
    state: S,
    snapshot_length: u64,
    snapshot: S,
}

#[derive(Clone)]
pub struct UserMessageIdMap {
    table: HashMap<String, u64>,
}

impl UserMessageIdMap {
    pub fn get(&self, client_id: &String) -> Option<u64> {
        self.table.get(client_id).cloned()
    }
}

impl StateMachine for UserMessageIdMap {
    fn new() -> Self {
        UserMessageIdMap {
            table: HashMap::new(),
        }
    }

    fn apply(&mut self, cmd: &Command) {
        self.table.insert(cmd.client_id.clone(), cmd.message_id);
    }
}

impl<S> SMWrapper<S>
where
    S: StateMachine,
    S: Clone,
{
    pub fn new(wal: WAL) -> Self {
        let mut state: S = StateMachine::new();
        state.apply_entries(wal.as_slice());
        SMWrapper {
            wal: wal,
            state: state,
            snapshot_length: 0,
            snapshot: StateMachine::new(),
        }
    }

    pub fn wal(&self) -> &WAL {
        &self.wal
    }

    pub fn take_snapshot(&mut self, len: u64) {
        let snapshot_length = self.snapshot_length;
        if snapshot_length <= len {
            self.snapshot_length = len;
            self.snapshot
                .apply_entries(&self.wal.as_slice()[snapshot_length as usize..len as usize]);
        } else {
            panic!();
        }
    }

    pub fn propose_entry(&mut self, entry: Entry) -> u64 {
        // must update state machine before proposing
        self.state
            .apply(entry.command.as_ref().expect("Apply command is None"));

        // return appended index
        return self.wal.propose_entry(entry);
    }

    pub fn append_entries(
        &mut self,
        prev_length: u64,
        prev_term: u64,
        entries: &[Entry],
    ) -> Option<u64> {
        let action = self.wal.append_entries(prev_length, prev_term, entries);

        match action {
            Some(Action::Update(l, entries)) => {
                let snapshot_length = self.snapshot_length;
                if snapshot_length <= l {
                    self.state = self.snapshot.clone();
                    self.state
                        .apply_entries(&self.wal.as_slice()[snapshot_length as usize..]);
                    Some(l + entries.len() as u64)
                } else {
                    panic!();
                }
            }
            Some(Action::Id(n)) => Some(n),
            None => None,
        }
    }

    pub fn state(&self) -> &S {
        &self.state
    }
}
