#!/usr/bin/env python3

import sys

if len(sys.argv) != 4 and len(sys.argv) != 6:
  print("Need 3 or 5 arguments: <BEGINNING_LINE> <BEGINNING_COLUMN> (next|to <TARGET_LINE> <TARGET_COLUMN>)", file=sys.stderr)
  sys.exit(1)


def lazy_read_stdin():
  """
  Lazily read stdin, characters per characters until EOF occurs.
  """
  while (line := sys.stdin.readline()):
    for c in line:
      yield c


_, begin_line, begin_column, command, *rem = sys.argv
if command == "to":
  target_line, target_column = map(int, rem)
else:
  target_line = 0
  target_column = 0
begin_line = int(begin_line)
begin_column = int(begin_column)


# TODO: handle the `to` command


# Inside a string
STATE_STRING = 0
# Found a backslash inside a string
STATE_STRING_BACKSLASH = STATE_STRING + 1
# Inside a comment (comments can be nested)
STATE_COMMENT = STATE_STRING_BACKSLASH + 1
# Beginning parenthese of comment encountered
STATE_BEGIN_COMMENT = STATE_COMMENT + 1
# Ending star of comment encountered
STATE_END_COMMENT = STATE_BEGIN_COMMENT + 1
# Beginning of subproofs encountered
STATE_BEGIN_PROOF = STATE_END_COMMENT + 1
# We are inside a bullet
STATE_BULLET = STATE_BEGIN_PROOF + 1

# The state stack, used to push/pop syntactic classes like strings, comments, etc
states = []

state_line = begin_line
state_column = begin_column
at_beginning_of_coq_line = True
any_found = False


def reached_target():
  """
  Returns `True` when the target position has been reached, else `False`.
  """
  return state_line > target_line or (state_line == target_line and state_column >= target_column)


def yield_position():
  """
  Print the current position as `<line>.<col>,<line>.<col>`.
  Return `True` if we should end the main loop, else `False` to continue processing.
  """
  global begin_line, begin_column, any_found

  # print the current range to stdout
  print(f'{begin_line}.{begin_column},{state_line}.{state_column} ', end='')
  begin_line = state_line
  begin_column = state_column + 1
  any_found = True

  return command == "next" or (command == "to" and reached_target())

# Iterate through all the characters from stdin
for char in lazy_read_stdin():
#  print(f'{state_line}:{state_column} {char}: [{" ".join(map(str, states))}]', file=sys.stderr)
  
  last_known_state = states[-1] if len(states) > 0 else None
  
  if char == '"':
    if last_known_state in [STATE_STRING, STATE_BEGIN_COMMENT, STATE_END_COMMENT]:
      # If we encounter `"` and we are either in a string, starting or ending a comment
      # then just pop the last state
      states.pop()
    elif last_known_state not in [STATE_STRING_BACKSLASH, STATE_COMMENT]:
      # If the last known state is not encountering a `\` in a string, or being
      # inside a comment, then we are going inside a string
      states.append(STATE_STRING)
    elif last_known_state == STATE_STRING_BACKSLASH:
      # If the last known state is encountering a `\`, simply pop it
      states.pop()
  elif char == '(':
    # When we encounter a `(`, if we are not in a string, then try to start
    # a comment
    if last_known_state in [STATE_BEGIN_COMMENT, STATE_END_COMMENT]:
      states.pop()
    if last_known_state != STATE_STRING:
      states.append(STATE_BEGIN_COMMENT)
  elif char == ')' and last_known_state == STATE_END_COMMENT:
    states.pop()
    states.pop()
  elif char == '*':
    # If we encounter a `*`, then:
    # - if we are inside a comment, then try starting the end of the comment
    # - if we are at the beginning of a line, then treat as a bullet
    # - if we are right after a `(`, then start a comment
    if last_known_state == STATE_BEGIN_COMMENT:
      states.pop()
      states.append(STATE_COMMENT)
    elif last_known_state == STATE_COMMENT:
      states.append(STATE_END_COMMENT)
    elif at_beginning_of_coq_line and last_known_state != STATE_BULLET:
      states.append(STATE_BULLET)
      at_beginning_of_coq_line = True
  elif char == '.':
    # If we encounter a `.` and we are not inside a string or a comment
    # treat it as the end of a coq statement
    if last_known_state not in [STATE_COMMENT, STATE_STRING]:
      at_beginning_of_coq_line = True
      if yield_position():
        break
    elif last_known_state in [STATE_BEGIN_COMMENT, STATE_END_COMMENT, STATE_STRING_BACKSLASH]:
      states.pop()
  elif char in ['-', '+'] and at_beginning_of_coq_line and last_known_state not in [STATE_COMMENT, STATE_END_COMMENT, STATE_STRING, STATE_STRING_BACKSLASH]:
    if last_known_state != STATE_BULLET:
      # If we are not already in a bullet, go into it
      states.append(STATE_BULLET)
    at_beginning_of_coq_line = True
  elif char == '{' and last_known_state not in [STATE_STRING, STATE_COMMENT, STATE_END_COMMENT, STATE_STRING_BACKSLASH]:
    # We are starting a new subproof
    states.append(STATE_BEGIN_PROOF)
    at_beginning_of_coq_line = True
    if yield_position():
      break
  elif char == '}' and last_known_state == STATE_BEGIN_PROOF:
    # We are ending a subproof
    states.pop()
    at_beginning_of_coq_line = True
    if yield_position():
      break
  elif last_known_state in [STATE_STRING_BACKSLASH, STATE_BEGIN_COMMENT, STATE_END_COMMENT]:
    states.pop()
  elif last_known_state == STATE_BULLET:
    state_column -= 1
    if yield_position():
      break
    state_column += 1
    states.pop()

  
  # if we ended a comment, do not flip the variable `at_beginning_of_coq_line`
  # (we may have written `something. (*  *) - something_else`, in which case the `-` is at the beginning
  # of the statement)
  if char == ')' and last_known_state == STATE_END_COMMENT or last_known_state == STATE_COMMENT:
    pass
  # When character is not a end of statement or a space, we are not at the beginning
  # of a coq statement anymore
  elif char not in ['.', ' ', '\t', '-', '*', '+', '{', '}']:
    at_beginning_of_coq_line = False

  if char == '\n':
    state_line += 1
    state_column = 1
  else:
    state_column += 1
else:
  if not any_found:
    yield_position()

# Commit all found positions
print()

# Exit successfully
sys.exit(0)
