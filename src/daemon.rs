use bimap::BiMap;

pub enum ConnectionState {
    Connected,
    Disconnected,
    Error,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Range {
    begin_line: u64,
    begin_column: u64,
    end_line: u64,
    end_column: u64,
}

pub struct DaemonState {
    connection: ConnectionState,

    root_id: i64,
    current_id: i64,

    states: BiMap<Range, i64>,
}

impl Default for DaemonState {
    fn default() -> Self {
        DaemonState {
            connection: ConnectionState::Disconnected,
            root_id: -1,
            current_id: -1,
            states: BiMap::new(),
        }
    }
}

impl DaemonState {
    pub fn set_root_id(&mut self, id: i64) {
        self.root_id = id;
    }

    pub fn set_current_id(&mut self, id: i64) {
        self.current_id = id;
    }
}
