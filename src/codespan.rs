use std::cmp::Ordering;

/// A very simply code span structure containing beginning and ending positions.
#[derive(PartialEq, Eq, Ord, Hash, Debug, Clone, Copy)]
pub struct CodeSpan {
    pub begin_line: u64,
    pub begin_column: u64,
    pub end_line: u64,
    pub end_column: u64,
}

impl CodeSpan {
    /// Creates a new code span.
    pub fn new(begin_line: u64, begin_column: u64, end_line: u64, end_column: u64) -> Self {
        Self {
            begin_line,
            begin_column,
            end_line,
            end_column,
        }
    }

    /// Extends a range using another one, only if it grows.
    /// For example, given a span `1.1,5.2`:
    /// - if it is extended with `1.1,6.3`, then the resulting range is `1.1,6.3`.
    /// - if it is extended with `1.1,3.5`, the resulting range is `1.1,5.2`.
    pub fn extend(self, other: &CodeSpan) -> Self {
        Self {
            begin_line: if other.begin_line <= self.begin_line {
                other.begin_line
            } else {
                self.begin_line
            },
            begin_column: if other.begin_line < self.begin_line {
                other.begin_column
            } else if other.begin_line == self.begin_line {
                if other.begin_column <= self.begin_column {
                    other.begin_column
                } else {
                    self.begin_column
                }
            } else {
                self.begin_column
            },
            end_line: if other.end_line >= self.end_line {
                other.end_line
            } else {
                self.end_line
            },
            end_column: if other.end_line > self.end_line {
                other.end_column
            } else if other.end_line == self.end_line {
                if other.end_column >= self.end_column {
                    other.end_column
                } else {
                    self.end_column
                }
            } else {
                self.end_column
            },
        }
    }
}

impl Default for CodeSpan {
    /// The default range is `1.1,1.1`.
    fn default() -> Self {
        Self {
            begin_line: 1,
            begin_column: 1,
            end_line: 1,
            end_column: 1,
        }
    }
}

impl PartialOrd for CodeSpan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.begin_line < other.begin_line {
            Some(Ordering::Less)
        } else if self.begin_line == other.begin_line {
            if self.begin_column < other.begin_column {
                Some(Ordering::Less)
            } else if self.begin_column == other.begin_column {
                Some(Ordering::Equal)
            } else {
                Some(Ordering::Equal)
            }
        } else {
            Some(Ordering::Greater)
        }
    }
}

impl std::fmt::Display for CodeSpan {
    /// Outputs a range using the format `<begin_line>.<begin_column>,<end_line>.<end_column>`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{},{}.{}",
            self.begin_line, self.begin_column, self.end_line, self.end_column
        )
    }
}
