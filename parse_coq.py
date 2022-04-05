import sys
from collections import deque
from dataclasses import dataclass, field
import time


@dataclass
class State:
    """
    The base class for all state representation in the state machine.
    """
    pass


@dataclass
class StateString(State):
    """
    A state indicating that the state machine feel inside a `"`-delimited string.
    """
    pass


@dataclass
class StateStringBackslash(State):
    """
    A state for when we are inside a string but we encountered a `\`.
    """
    pass


@dataclass
class StateComment(State):
    """
    A state which indicates that the reader is inside a comment.
    A comment has the general shape `(*<...>*)` where `<...>` is any sequence of characters
      (including `*` and `)` as long as they are not next to each other).
    """
    at_beginning_of_coq_line: bool = field(default=False)


@dataclass
class StateBeginComment(State):
    """
    This state indicates that we have found a `(`, but we have yet to decide
      whether we found an actual comment (when `*` succedes) or not.
    """
    at_beginning_of_coq_line: bool = field(default=False)


@dataclass
class StateEndComment(State):
    """
    We are in a comment and we found a `*`.
    Whether this comment is to be ended or not is yet to be decided from the next character read
      (if it is `)` or not).
    """
    at_beginning_of_coq_line: bool = field(default=False)


@dataclass
class StateBeginProof(State):
    """
    UNUSED
    """
    pass


@dataclass
class StateEOL(State):
    """
    We have encountered a `.`, but it is located right after an alphanumerical character.
    This may be a qualified identifier!
    This will be decided from the next character read (if it is an alphabetical character).
    """
    pass


@dataclass
class StateBullet(State):
    """
    A state indicating that we are currently parsing a Coq bullet, to start a new subproof.

    Attributes:
      bullet_style (str): the current style of bullet (one of `+`, `*`, `-`) which has already been parsed.
    """
    bullet_style: str


@dataclass
class StateWS(State):
    """
    We have encountered a whitespace, which if followed by a `.` indicates a statement end.
    """
    pass


@dataclass
class StateWSEOL(State):
    """
    We have encountered a whitespace, which if followed by a `.` indicates a statement end.
    """
    pass


def lazy_read_stdin():
    """
    Lazily read stdin, characters per characters until EOF occurs.

    This is useful as reading the entire stdin may yield a very big string, which may
      take too much time to load/create.
    Instead, performance is rather predictible because if the end of a statement is at
      the same position in a stream of 50 to multiple billions of characters,
      the same number of characters should have been read in the end.
    """
    while (line := sys.stdin.readline()):
        for c in line:
            yield c


def escape(code):
    """
    Escape a slice of Coq code to make it safe to include in `"`-enclosed strings.
    """
    return code.replace('"', '\\"').replace('\n', '\\n')


def reached_target():
    """
    Checks whether we have reached our target or not (if command is `to`).
    If command is `next`, always return `True`.
    """

    global command, current_line, current_column, target_line, target_column

    if command == 'next':
        return True
    if current_line > target_line or (current_line == target_line
                                      and current_column >= target_column):
        # Reaching the target means either:
        # - the current line has gone past the target line, in which case our target has clearly been reached.
        # - the current line is equal to the target line, but the current column has gone past the target column.
        return True
    return False


def yield_position():
    """
    Send the current position, along with the code, following the format
      `{b_line}.{b_col},{e_line}.{e_col} "{escaped code}"`.
    """
    global begin_line, begin_column, current_line, current_column, any_found, code, at_beginning_of_coq_line

    print(
        f'{begin_line}.{begin_column},{current_line}.{current_column} "{escape(code)}"'
    )
    sys.stdout.flush()

    #current_column += 1
    begin_line = current_line
    begin_column = current_column + 1
    code = ""
    any_found = True
    at_beginning_of_coq_line = True

    return reached_target()


def parse(c):
    """
    Try to parse the character given from the current state queue.
    This is basically a state machine.

    Args:
      c (str): The character to process on this iteration of the loop.
    """

    global states, code, current_line, current_column, at_beginning_of_coq_line

    state = states[0] if states else None
    current_state = type(state) if state is not None else None

    if current_state is StateEOL:
        # We have encountered a `.` just after this character.
        # This means that if `c` is a non-letter character, then we have found a Coq statement
        #   else it is most likely a qualified identifier.
        if c.isalpha():
            # We are parsing a qualified identifier. Don't stop there, just pop the current state.
            states.popleft()
        elif c == '.':
            # Wait, `..` is a valid identifier! Skip this second `.` then...
            states.popleft()
        else:
            # Here we are, we have found a non-letter character.
            # This means that our Coq statement is finished.
            #
            # 1. Remove the last character inputted (just in case it isn't a whitespace)
            # 2. Backtrack one column
            # 3. Yield the position
            # 4. Move forward one column
            # 5. Tell that we are beginning a new Coq statement
            # 6. Add the character removed (which is `c`) and try parsing again from it
            # 7. Remove the last state: we are not checking if we reached EOL anymore after this
            code, current_column = code[:-1], current_column - 1
            if yield_position():
                return False
            code, current_column = code + c, current_column + 1
            states.popleft(
            )  # NOTE: the whole `if-else` can be reduced to only an `if`

            return parse(c)

    elif current_state is StateString:
        # We are inside of a string.
        # The only way to escape the string is to have `c == '"'`, which immediately
        #   ends the current string.
        if c == '"':
            # We have found the `"` character, so we can end the string right here right now.
            states.popleft()
        elif c == '\\':
            # Hold on, we have found a `\` inside a string.
            # The next character must be ignored at all cost!
            states.appendleft(StateStringBackslash())
        # If any other character is found, do nothing because it will not impact the current state.

    elif current_state is StateStringBackslash:
        # We have seen a `\` at the last iteration, which means that this character is meaningless
        #   (may it be an error, Coq wil report it).
        # So we can simply pop this intermediate state.
        states.popleft()

    elif current_state is StateComment:
        # We are currently parsing a comment. However, those are tricky to handle.
        # Basically, a comment has the general shape `(*<...>*)` where `<...>` can be composed of any character
        #   (including `*` and `)` as long as they are not next to each other).
        #
        # - So if we encounter a `*`, we need to jump to an intermediate state which will handle
        #   whether the comment has been ended or not.
        #   Also, we need to record whether the comment was currently at the beginning of a Coq statement
        #   in order not to forget about it and mess everything up.
        # - For any other character, simply continue as they will not impact the current state.
        if c == '*':
            states.appendleft(StateEndComment(state.at_beginning_of_coq_line))

    elif current_state is StateBeginComment:
        # A `(` has been seen just before, but we don't currently know if we are really starting a comment or not.
        # We have to see a `*` right now for it to be considered a comment opening.
        # In case none was seen, simply pop this state because it is garbage.
        if c == '*':
            # Yay! There's a `*`, so let's start a new comment here.
            #
            # 1. Pop the current state because it is now garbage
            # 2. Push a new comment state, recording if we are at the beginning of a line
            st = states.popleft()
            states.appendleft(StateComment(st.at_beginning_of_coq_line))
        else:
            # Because we are not opening a new comment, we know that we are not at the beginning of
            #   a Coq statement anymore.
            states.popleft()
            at_beginning_of_coq_line = False

    elif current_state is StateEndComment:
        # Hey! We are inside a comment but a `*` has been seen right before this.
        #
        # - If the current character is a `)`, we need to pop the comment state and this one.
        #   Don't forget to also restore information about the beginning of the line!
        # - Else we can just pop this state as it is garbage.
        if c == ')':
            st = states.popleft()
            states.popleft(
            )  # NOTE: both comment states contain the information we need
            #   so `st =` can really be for any of those two.
            at_beginning_of_coq_line = st.at_beginning_of_coq_line
        else:
            states.popleft()

    elif current_state is StateBullet:
        # Can we continue parsing a bullet?
        # Yes, if the next character is the same style as the currently parsed bullet.
        #
        # However, if it is not, just backtrack, yield the position,
        #   go forward and try parsing again from this position.
        if c == state.bullet_style:
            # We have found the current bullet style, so let's continue trying to parse
            #   a bullet.
            pass
        else:
            # Stop the current bullet, backtrack and yield the position, assume we are
            #   at the beginning of a line, and then trying parsing again from the current character.
            code, current_column = code[:-1], current_column - 1
            if yield_position():
                return False
            code, current_column = code + c, current_column + 1
            states.popleft()

            return parse(c)

    elif current_state is StateWS:
        # There is a whitespace right before, but we still need to decide if we are ending a
        #   statement (if it is followed by a `.`) or not.
        #
        # - If `.` is the current character, end a Coq statement right here.
        # - Otherwise, pop the state and try parsing again the current character within the old state.
        if c == '.':
            # Yay! We can finally end the current statement. We did it!
            # Unless this is the beginning of a `..`...
            states.popleft()
            states.appendleft(StateWSEOL())
        else:
            # Ok so just ignore this garbage state, and restart parsing on the current character.
            states.popleft()
            return parse(c)

    elif current_state is StateWSEOL:
        # We have encountered both a whitespace and a `.`.
        # However, if we found yet another `.`, then we are not ending a statement!
        if c == '.':
            states.popleft()
        else:
            # No `.` found, so end the current statement and start fresh if needed.
            if yield_position():
                return False
            states.popleft()
            return parse(c)

    else:
        # There is no state in the state machine.
        # We perform here a case analysis on some characters (e.g. `(` or `{`) to see if they
        #   create new states or not.
        #
        # - `(` pushes a `StateBeginComment` because we may start a new comment here.
        # - `{` immediately yields the position as this starts a new bullet style
        #   (new subproof, must be ended by `}`).
        #
        #   TODO: this has to be done only within `Proof. ??? Qed.`! Because the same
        #     symbol is used to denote implicit parameters.
        #     We may simply not support these, for simplicity.
        # - `}` yields the current position as it stops the current subproof.
        # - `*`, `-`, `+` all start a new bullet ONLY if at the beginning of a line.
        # - `"` starts a string (pushes a `StateString`).
        # - `.` jumps to a state where we want to determine if we are parsing a qualified identifier
        #   or the end of a statement.
        # - ` `, `\t` push a new `StateWS` in case we are ending a statement.
        # - Otherwise, ignore and continue not at the beginning of a line.
        if c == '(':
            states.appendleft(StateBeginComment(at_beginning_of_coq_line))
        elif c in ['*', '+', '-'] and at_beginning_of_coq_line:
            states.appendleft(StateBullet(c))
        elif c == '"':
            states.appendleft(StateString())
        elif c == '.':
            states.appendleft(StateEOL())
        elif c.isspace():
            states.appendleft(StateWS())
        else:
            at_beginning_of_coq_line = False

    # In any case, because we have read a new character, we need to increment the current location.
    # Attention: if the current character is a `\n`, we need to increment the line, not the column
    #   and set the column to 1 (emulating a carriage return).
    if c == '\n':
        current_line, current_column = current_line + 1, 1
    else:
        current_column += 1

    return True


if __name__ == '__main__':
    if len(sys.argv) not in [4, 6]:
        print(
            "Need 3 or 5 arguments: <BEGINNING_LINE> <BEGINNING_COLUMN> (next|to <TARGET_LINE> <TARGET_COLUMN>)",
            file=sys.stderr)
        sys.exit(1)

    begin_line, begin_column, command, *rem = [
        *map(int, sys.argv[1:3]), *sys.argv[3:]
    ]
    if command == "to":
        target_line, target_column = map(int, rem)
    else:
        target_line, target_column = 0, 0

    states = deque()

    current_line = begin_line
    current_column = begin_column
    at_beginning_of_coq_line = True
    any_found = False
    code = ""

    for c in lazy_read_stdin():
        code += c

        must_continue = parse(c)
        if not must_continue:
            break

    # TODO: seems to read way too much characters.
    # Also, we might just want not to proceed further as no statement is currently finished.
    else:
        #     if not any_found:
        #         yield_position()
        pass

    sys.exit(0)
