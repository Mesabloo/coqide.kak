use std::fmt;

#[derive(Clone, Debug, Copy)]
pub struct Range {
    begin: (u64, u64),
    end: (u64, u64),
}

impl Range {
    pub fn new(begin_line: u64, begin_column: u64, end_line: u64, end_column: u64) -> Self {
        Self {
            begin: (begin_line, begin_column),
            end: (end_line, end_column),
        }
    }
}

impl Default for Range {
    fn default() -> Self {
        Self {
            begin: (1, 1),
            end: (1, 1),
        }
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{},{}.{}",
            self.begin.0, self.begin.1, self.end.0, self.end.1
        )
    }
}
