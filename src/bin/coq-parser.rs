#![feature(stdin_forwarders)]

use std::{
    env,
    io::{self, Write},
    process,
};

/// Coq's bullet styles.
///
/// `{` and `}` are not in there because they do not stack (meaning `{{` is considered two different bullets).
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
enum BulletStyle {
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
}

impl BulletStyle {
    /// Gets the [`BulletStyle`] corresponding to a given character.
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '+' => Some(Self::Plus),
            '-' => Some(Self::Minus),
            '*' => Some(Self::Star),
            _ => None,
        }
    }
}

/// An enumeration describing all states of the parser's state machine.
#[derive(Clone, Debug)]
enum MachineState {
    /// The current cursor is inside a string (a `"`-separated sequence of characters).
    InString,
    /// A backslash character `\` has been encountered before while inside a string.
    BackslashInString,
    /// The cursor lies inside a Coq comment, which have the general shape `(*<...>*)` where `<...>` is
    /// any sequence of characters (including end of lines).
    InComment {
        /// Was the cursor at the beginning of a Coq statement before?
        at_beginning_of_coq_line: bool,
    },
    /// The cursor found a `(`, but the whole state machine does not yet know if it starts a comment or not.
    AtBeginningOfComment { at_beginning_of_coq_line: bool },
    /// The cursor is currently inside a comment, but it read `*` just earlier, and we don't yet knwo if
    /// it finishes the current comment or not.
    AtEndOfComment { at_beginning_of_coq_line: bool },
    /// We have found a `.` which ends a Coq statement.
    AtEOL,
    /// We have started parsing a bullet (either `-`, `*` or `+` repeated at least once) earlier.
    InBullet {
        /// The current bullet style.
        bullet_style: BulletStyle,
    },
    /// If we find a `.` after a whitespace, it can either be the end of a Coq statement, or the identifier `..`
    /// used in notations.
    AfterWhitespace,
    /// Records when we have found both ` .` just before, but we still need to decide if we found a `.` or a `..`.
    AfterWhitespaceAndEOL,
}

/// Records the position we are currently analyzing.
#[derive(Clone, Copy, Debug)]
struct Cursor(
    /// The line number.
    u64,
    /// The column number.
    u64,
);

impl Cursor {
    fn new(line: u64, column: u64) -> Self {
        Cursor(line, column)
    }

    fn step(&mut self, c: char) {
        if c == '\n' {
            self.0 += 1;
            self.1 = 1;
        } else {
            self.1 += 1;
        }
    }

    fn back(&mut self) {
        self.1 -= 1;
    }
}

/// All available CLI commands.
enum Command {
    /// Only return the bounds of the next Coq statement.
    Next,
    /// Return all the bounds of the next Coq statements until we reach a given point.
    To { target: Cursor },
}

/// The global machine state.
struct GlobalState {
    /// The current position we are analyzing in the stream.
    /// This is updated each time we fetch a new character.
    cursor: Cursor,
    /// All the code which has been processed up until now.
    code: String,
    /// The starting position in the original buffer.
    starting: Cursor,
    /// The command used to start this program.
    command: Command,
    /// Has there been any Coq statement found yet?
    any_found: bool,
    /// Are we at the beginning of a Coq statement?
    at_beginning_of_coq_line: bool,
    /// The current state stack.
    states: Vec<MachineState>,
}

impl GlobalState {
    fn new(starting: Cursor, command: Command) -> Self {
        Self {
            cursor: starting,
            code: "".to_string(),
            starting,
            command,
            any_found: false,
            at_beginning_of_coq_line: true,
            states: Vec::new(),
        }
    }

    fn push_state(&mut self, st: MachineState) {
        self.states.push(st);
    }

    fn pop_state(&mut self) -> Option<MachineState> {
        self.states.pop()
    }

    fn append_char(&mut self, c: char) {
        self.code.push(c);
    }

    fn next(&mut self, c: char) {
        self.cursor.step(c);
    }

    fn backtrack_once(&mut self) {
        self.cursor.back();
        let _ = self.code.pop();
    }
}

/// Escapes a given string slice to make it safe to use with the plugin.
///
/// - `\n` is substituted for `\\n`
/// - `\"` is replaced with `\\\"`
fn escape(code: &String) -> String {
    code.replace("\n", "\\n").replace("\"", "\\\"")
}

fn reached_target(st: &GlobalState) -> bool {
    match &st.command {
        Command::Next => true,
        Command::To { target } => {
            st.cursor.0 > target.0 || (st.cursor.0 == target.0 && st.cursor.1 >= target.1)
        }
    }
}

fn yield_position(st: &mut GlobalState) -> bool {
    println!(
        "{}.{},{}.{} \"{}\"",
        st.starting.0,
        st.starting.1,
        st.cursor.0,
        st.cursor.1,
        escape(&st.code)
    );
    io::stdout().flush().unwrap();

    st.starting = st.cursor.clone();
    st.starting.step('\0');
    st.code = "".to_string();
    st.any_found = true;
    st.at_beginning_of_coq_line = true;

    reached_target(st)
}

/// Tries to parse the given character in the current state.
///
/// Returns `true` if processing stops here.
fn parse(c: char, st: &mut GlobalState) -> bool {
    let last_state = st.states.last().cloned();

    match last_state {
        Some(MachineState::AtEOL) => {
            // We have encountered a `.` at the last iteration.
            // However, it is located right after a letter or a number, which makes it
            // a candidate for a qualified identifier instead of a statement end.
            //
            // Therefore, if the current character is alphabetic, then we are starting a qualified identifier,
            // which means that we must ignore this `.`.
            if c.is_alphabetic() {
                let _ = st.pop_state().unwrap();
            } else if c == '.' {
                // Hold on, `..` is in fact a valid Coq identifier (used in notations, for repetitive operators).
                // So we have to skip it.
                let _ = st.pop_state().unwrap();
            } else {
                // Else inform that we have found a complete Coq statement.
                st.backtrack_once();
                if yield_position(st) {
                    return false;
                }
                st.append_char(c);
                st.next('\0');

                let _ = st.pop_state().unwrap();

                return parse(c, st);
            }
        }
        Some(MachineState::InString) => {
            // We are inside of a `"`-delimited string, which ends only if the current character
            // is a `"`.
            // Treat `\` specially when one is found inside a string.
            match c {
                '"' => {
                    let _ = st.pop_state().unwrap();
                }
                '\\' => {
                    let _ = st.push_state(MachineState::BackslashInString);
                }
                _ => {}
            }
            st.at_beginning_of_coq_line = false;
        }
        Some(MachineState::BackslashInString) => {
            // We have found a `\` earlier, which means that we must ignore the current character.
            // Simply pop the last machine state.
            let _ = st.pop_state().unwrap();
        }
        Some(MachineState::InComment {
            at_beginning_of_coq_line,
        }) => {
            // The cursor points to within a comment, which can end only if we find a `*`.
            if c == '*' {
                st.push_state(MachineState::AtEndOfComment {
                    at_beginning_of_coq_line,
                });
            }
        }
        Some(MachineState::AtBeginningOfComment {
            at_beginning_of_coq_line,
        }) => {
            // We have seen a `(` just before, and are looking towards seeing `*` to begin a comment.
            // In case it does not happen, we can simply ignore the current character and treat the last
            // character as a normal character.
            if c == '*' {
                let _ = st.pop_state().unwrap();
                st.push_state(MachineState::InComment {
                    at_beginning_of_coq_line,
                });
            } else {
                let _ = st.pop_state().unwrap();
                st.at_beginning_of_coq_line = false;
            }
        }
        Some(MachineState::AtEndOfComment {
            at_beginning_of_coq_line,
        }) => {
            // We are currently inside a comment, and we saw a `*` just before.
            // But if we do not come across a `)`, then the comment will not end right now.
            if c == ')' {
                let _ = st.pop_state();
                let _ = st.pop_state();
                st.at_beginning_of_coq_line = at_beginning_of_coq_line;
            } else {
                let _ = st.pop_state();
            }
        }
        Some(MachineState::InBullet { bullet_style }) => {
            // We have started parsing a bullet, which may be unterminated if the current character as the
            // same bullet style.
            //
            // If the character is not a bullet character, or the style is different, then we found a
            // Coq statement and we must end here.

            if BulletStyle::from_char(c) != Some(bullet_style) {
                st.backtrack_once();
                if yield_position(st) {
                    return false;
                }
                st.append_char(c);
                st.next('\0');

                let _ = st.pop_state().unwrap();

                return parse(c, st);
            }
        }
        Some(MachineState::AfterWhitespace) => {
            // There was a whitespace right before.
            // If it is followed by a `.`, then this is most likely the end of a Coq statement.
            // However, in such case, we cannot yet decide as it could be a single `..` identifier.
            if c == '.' {
                let _ = st.pop_state().unwrap();
                st.push_state(MachineState::AfterWhitespaceAndEOL);
                st.at_beginning_of_coq_line = false;
            } else {
                let _ = st.pop_state().unwrap();
                return parse(c, st);
            }
        }
        Some(MachineState::AfterWhitespaceAndEOL) => {
            // There is a  ` .` sequence right before, but we still have to decide if we want to end
            // the Coq statement now, or not.
            // If we find a `.` right now, then we really don't want to end it.
            // Else, we simply will.
            if c == '.' {
                let _ = st.pop_state().unwrap();
                st.at_beginning_of_coq_line = false;
            } else {
                if yield_position(st) {
                    return false;
                }

                let _ = st.pop_state().unwrap();
                return parse(c, st);
            }
        }
        None => {
            // There is no state in the machine, so we have to perform a case analysis on the input character:
            //
            // - `(` pushes a transition state to determine whether we are starting a comment or not.
            // - `*`, `-` and `+` all start new bullets.
            // - `{` and `}` start non-stackeable bullets.
            // - `"` is the entry point of the string state.
            // - `.` jumps to a transition state to check if we found the identifier `..`, a qualified
            //   identifier or the end of a Coq statement.
            // - ` `, `\t` jump to the special whitespace state.
            //
            // For any character not in this list, simply ignore and continue in the middle of a Coq statement.
            //
            match c {
                '(' => st.push_state(MachineState::AtBeginningOfComment {
                    at_beginning_of_coq_line: st.at_beginning_of_coq_line,
                }),
                '"' => st.push_state(MachineState::InString),
                '.' => st.push_state(MachineState::AtEOL),
                '{' | '}' if st.at_beginning_of_coq_line => {
                    if yield_position(st) {
                        return false;
                    }
                }
                c if c.is_whitespace() => st.push_state(MachineState::AfterWhitespace),
                c => match BulletStyle::from_char(c) {
                    Some(style) if st.at_beginning_of_coq_line => {
                        st.push_state(MachineState::InBullet {
                            bullet_style: style,
                        })
                    }
                    _ => {
                        st.at_beginning_of_coq_line = false;
                    }
                },
            }
        }
    }

    st.next(c);

    true
}

fn main() {
    let args = env::args().collect::<Vec<_>>();

    if args.len() != 4 && args.len() != 6 {
        process::exit(-1);
    }

    let begin_line = args[1].parse::<u64>().unwrap();
    let begin_column = args[2].parse::<u64>().unwrap();
    let command = if args.len() == 4 {
        if args[3].as_str() == "next" {
            Command::Next
        } else {
            panic!("Invalid command '{}'", args[3])
        }
    } else {
        if args[3].as_str() == "to" {
            Command::To {
                target: Cursor::new(
                    args[4].parse::<u64>().unwrap(),
                    args[5].parse::<u64>().unwrap(),
                ),
            }
        } else {
            panic!("Invalid command '{}'", args[3])
        }
    };

    let mut global_state = GlobalState::new(Cursor::new(begin_line, begin_column), command);

    'global_loop: for line in io::stdin().lines() {
        match line {
            Err(_) => break,
            Ok(mut line) => {
                line.push('\n'); // we actually need them

                for ch in line.chars() {
                    global_state.append_char(ch);

                    let must_continue = parse(ch, &mut global_state);
                    if !must_continue {
                        break 'global_loop;
                    }
                }
            }
        }
    }
    if !global_state.any_found {
        // TODO: maybe we need to do something here, e.g. let the user know
        // that we were not able to determine a statement there.
    }

    process::exit(0);
}
