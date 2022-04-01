#!/usr/bin/env python3

import sys
from dataclasses import dataclass, field

if len(sys.argv) != 4 and len(sys.argv) != 6:
    print(
        "Need 3 or 5 arguments: <BEGINNING_LINE> <BEGINNING_COLUMN> (next|to <TARGET_LINE> <TARGET_COLUMN>)",
        file=sys.stderr)
    sys.exit(1)


def lazy_read_stdin():
    """
  Lazily read stdin, characters per characters until EOF occurs.
  """
    while (line := sys.stdin.readline()):
        # print(f'line read: {line.__repr__()}', file=sys.stderr)

        for c in line:
            yield c


@dataclass
class State: pass
@dataclass
class StateString(State): pass
@dataclass
class StateStringBackslash(State): pass
@dataclass
class StateComment(State):
  at_beginning_of_coq_line: bool = field(default = False)
@dataclass
class StateBeginComment(State):
  at_beginning_of_coq_line: bool = field(default = False)
@dataclass
class StateEndComment(State):
  at_beginning_of_coq_line: bool = field(default = False)
@dataclass
class StateBeginProof(State): pass
@dataclass
class StateEOL(State): pass
@dataclass
class StateBullet(State): pass

_, begin_line, begin_column, command, *rem = sys.argv
if command == "to":
    target_line, target_column = map(int, rem)
else:
    target_line = 0
    target_column = 0
begin_line = int(begin_line)
begin_column = int(begin_column)

# TODO: handle the `to` command

# The state stack, used to push/pop syntactic classes like strings, comments, etc
states = []

state_line = begin_line
state_column = begin_column
at_beginning_of_coq_line = True
any_found = False
code = ""

last_char = None


def reached_target():
    """
  Returns `True` when the target position has been reached, else `False`.
  """
    return state_line > target_line or (state_line == target_line
                                        and state_column >= target_column)


def escape_quotes(code):
  return code.replace('"', '\\"')


def yield_position():
    """
  Print the current position as `<line>.<col>,<line>.<col>`.
  Return `True` if we should end the main loop, else `False` to continue processing.
  """
    global begin_line, begin_column, any_found, code

    # print the current range to stdout
    print(f'{begin_line}.{begin_column},{state_line}.{state_column} "{escape_quotes(code)}"')
    sys.stdout.flush()
    
    begin_line = state_line
    begin_column = state_column + 1
    code = ""
    any_found = True

    return command == "next" or (command == "to" and reached_target())


# Iterate through all the characters from stdin
for char in lazy_read_stdin():
    code += char
    #print(f'{at_beginning_of_coq_line} - {state_line}:{state_column} {char}: [{" ".join(map(str, states))}]', file=sys.stderr)

    last_known_state = states[-1] if len(states) > 0 else None

    if type(last_known_state) is StateEOL:
        if char.isspace():
            state_column -= 1
            if yield_position():
                break
            state_column += 1
        at_beginning_of_coq_line = True
        last_known_state = states[-1] if len(states) > 0 else None

    if char == '"':
        if type(last_known_state) in [
                StateString, StateBeginComment, StateEndComment
        ]:
            # If we encounter `"` and we are either in a string, starting or ending a comment
            # then just pop the last state
            states.pop()
        elif type(last_known_state) not in [StateStringBackslash, StateComment]:
            # If the last known state is not encountering a `\` in a string, or being
            # inside a comment, then we are going inside a string
            states.append(StateString())
        elif type(last_known_state) is StateStringBackslash:
            # If the last known state is encountering a `\`, simply pop it
            states.pop()
    elif char == '(':
        # When we encounter a `(`, if we are not in a string, then try to start
        # a comment
        if type(last_known_state) in [StateBeginComment, StateEndComment]:
            states.pop()
        if type(last_known_state) is not StateString:
            states.append(StateBeginComment(at_beginning_of_coq_line))
    elif char == ')' and type(last_known_state) is StateEndComment:
        states.pop()
        states.pop()
        at_beginning_of_coq_line = last_known_state.at_beginning_of_coq_line
    elif char == '*':
        # If we encounter a `*`, then:
        # - if we are inside a comment, then try starting the end of the comment
        # - if we are at the beginning of a line, then treat as a bullet
        # - if we are right after a `(`, then start a comment
        if type(last_known_state) is StateBeginComment:
            states.pop()
            states.append(StateComment(last_known_state.at_beginning_of_coq_line))
        elif type(last_known_state) == StateComment:
            states.append(StateEndComment(last_known_state.at_beginning_of_coq_line))
        elif at_beginning_of_coq_line and type(last_known_state) is not StateBullet:
            states.append(StateBullet())
            at_beginning_of_coq_line = True
    elif char == '.':
        # If we encounter a `.` and we are not inside a string or a comment
        # treat it as the end of a coq statement IF either the character before or the character
        # after is blank (space, tab, newline, ...)
        if type(last_known_state) not in [StateComment, StateString]:
            if last_char is not None and not last_char.isspace():
                states.append(StateEOL())
            else:
                at_beginning_of_coq_line = True
                if yield_position():
                    break
        elif type(last_known_state) in [
                StateBeginComment, StateEndComment, StateStringBackslash
        ]:
            states.pop()
    elif char in ['-', '+'
                  ] and at_beginning_of_coq_line and type(last_known_state) not in [
                      StateComment, StateEndComment, StateString, StateStringBackslash
                  ]:
        if type(last_known_state) is not StateBullet:
            # If we are not already in a bullet, go into it
            states.append(StateBullet())
        at_beginning_of_coq_line = True
    elif char == '{' and type(last_known_state) not in [
            StateString, StateComment, StateEndComment, StateStringBackslash
    ] and at_beginning_of_coq_line:
        # We are starting a new subproof
        #states.append(StateBeginProof())
        at_beginning_of_coq_line = True
        if yield_position():
            break
    elif char == '}' and at_beginning_of_coq_line:
        # We are ending a subproof
        states.pop()
        at_beginning_of_coq_line = True
        if yield_position():
            break
    elif type(last_known_state) in [
        StateStringBackslash, StateBeginComment, StateEndComment
    ]:
        states.pop()
    elif type(last_known_state) == StateBullet:
        state_column -= 1
        if yield_position():
            break
        state_column += 1
        states.pop()

    # if we ended a comment, do not flip the variable `at_beginning_of_coq_line`
    # (we may have written `something. (*  *) - something_else`, in which case the `-` is at the beginning
    # of the statement)
    if char == ')' and type(last_known_state) in [StateEndComment, StateComment]:
        pass
    # If we encounter a newline, do not change the beginning_of_line state
    # because we may be at the beginning of a coq statement, or in the middle.
    elif char == '\n':
        pass
    # When character is not a end of statement or a space, we are not at the beginning
    # of a coq statement anymore
    elif char not in [
            '.', ' ', '\t', '-', '*', '+', '{', '}'
    ] and type(last_known_state) not in [StateComment, StateEndComment]:
        at_beginning_of_coq_line = False

    if char == '\n':
        state_line += 1
        state_column = 1
    else:
        state_column += 1

    last_char = char
else:
    if not any_found:
        yield_position()

# Commit all found positions
print()

# Exit successfully
sys.exit(0)
