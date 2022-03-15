# Interface:
# - coqide_command:    the command used to launch the backend
# - coqide-start:      start the backend daemon and try to connect to it
# - coqide-stop:       stop the daemon, give up on the processed state and remove logs
# - coqide-dump-log:   dump the logs into a specific buffer
# - coqide-next:       try to process the next Coq statement
# - coqide-previous:   try to go back to the last processed state
# - coqide-move-to:    move to the end of the next Coq statement (from the main cursor) and try to process until this point
# - coqide-hints:      output some hints in the result buffer
# - coqide-goto-tip:   move cursor to processed tip
# ------- Colors
# - coqide_processed:  a specific face to highlight what has been processed by CoqIDE
# - coqide_errors:     a face to highlight errors returned by CoqIDE
# - coqide_keyword:    how keywords are colored in goal/result buffers
# - coqide_evar:       how evars are colored in goal/result buffers
# - coqide_type:       how type are colored in goal/result buffers
# - coqide_notation:   how operators are colored in goal/result buffers
# - coqide_variable:   how variables are colored in goal/result buffers
# - coqide_reference:  how reference are colored in goal/result buffers
# - coqide_path:       how paths are colored in goal/result buffers

declare-option -docstring "
  Command to launch `coqide-daemon`.
" str coqide_command "coqide-daemon"

set-face global coqide_processed default,black
set-face global coqide_errors default,red

set-face global coqide_keyword @keyword
set-face global coqide_evar @variable
set-face global coqide_type @type
set-face global coqide_notation @operator
set-face global coqide_variable @variable
set-face global coqide_reference @variable
set-face global coqide_path @module

####################################################################################################

declare-option -docstring "
  The path to this file when sourced.
" -hidden str coqide_source %sh{
  dirname "$kak_source"
}

# Include our coq code highlighter
source "%opt{coqide_source}/coq-highlight.kak"
# Include plugin internals
# source "%opt{coqide_source}/internals.kak"


declare-option -docstring "
  The PID of the `coqide-daemon` process used to interact with kakoune.
" -hidden str coqide_pid

declare-option -docstring "
  The PID of the `socat` process used to send commands to the daemon.
" -hidden str coqide_socat_pid

declare-option -docstring "
  The path to the pipe used to control the `coqide-kak` process.

  This should /NOT/ be modified while the process is running, unless you want
  junk files on your system (putting them in `/tmp` should solve at least
  half of the problem).
" -hidden str coqide_pipe

declare-option -docstring "
  The buffer in which we are editing Coq source code.
" -hidden str coqide_buffer 

declare-option -docstring "
  The range to be highlighted in the buffer, which indicates how much the buffer has been processed by `coqidetop`.

  The face used to highlight can be customised with the option `coqide_processed`.
" -hidden range-specs coqide_processed_range

declare-option -docstring "
  The range to be highlighted in the buffer, which indicates that an error happened when processing the range.

  The face can be customized with the face `coqide_errors`.
" -hidden range-specs coqide_error_range

declare-option -docstring "
  The name of the buffer used to show logs of the daemon.

  To allow for concurrent daemons, it is formatted as `*coqide-%%pid-log*`.
" -hidden str coqide_log_buffer

declare-option -docstring "
  The name of the goal buffer for the current coqide daemon.

  To allow for concurrent daemons, it is formatted as `*coqide-%%pid-goal*`.
" -hidden str coqide_goal_buffer

declare-option -docstring "
  The name of the result buffer for the current coqide daemon.

  To allow for concurrent daemons, it is formatted as `*coqide-%%pid-result*`.
" -hidden str coqide_result_buffer

declare-option -docstring "
  Ranges to highlight in the result buffer. 
" -hidden range-specs coqide_result_highlight

declare-option -docstring "
  Ranges to highlight in the goal buffer.
" -hidden range-specs coqide_goal_highlight


define-command -docstring "
  Start `coqide-kak` for the current buffer.  
" -params 0 coqide-start %{
  evaluate-commands %sh{
    if [ -n "$kak_opt_coqide_pid" ]; then
      echo 'fail "coqide: already started in buffer"'
    fi
  }
  
  set-option buffer coqide_buffer %val{buffile}
  set-option buffer coqide_processed_range %val{timestamp} '1.1,1.1|coqide_processed'
  set-option buffer coqide_error_range %val{timestamp}
  
  set-option buffer coqide_pipe %sh{
    filename="${kak_opt_coqide_buffer//[^A-Za-z0-9._-]/_}"
    echo "/tmp/coqide-${kak_session}-pipe-${filename}"
  }

  nop %sh{
    mkdir -p "$kak_opt_coqide_pipe"
  }

  evaluate-commands %sh{
    if ! type "$kak_opt_coqide_command" &>/dev/null; then
      echo "fail 'coqide: Unknown command \"$kak_opt_coqide_command\"'"
      exit
    fi
  }
  
  set-option buffer coqide_pid %sh{
    env RUST_BACKTRACE=1 "$kak_opt_coqide_command" "$kak_session" "$kak_opt_coqide_buffer" "$kak_opt_coqide_pipe" </dev/null &>"$kak_opt_coqide_pipe/log" &
    echo "$!"
  }

  set-option buffer coqide_log_buffer "*coqide-%opt{coqide_pid}-log*"
  set-option buffer coqide_goal_buffer "*coqide-%opt{coqide_pid}-goal*"
  set-option buffer coqide_result_buffer "*coqide-%opt{coqide_pid}-result*"

  evaluate-commands %sh{
    # Ideally, there is a way to add a hook on buffer modifications, but there seems
    # to be none at the moment, which is unfortunate
    echo "
      hook -once -group coqide buffer=$kak_opt_coqide_buffer BufClose .* %{ coqide-stop }
      hook -once -group coqide buffer=$kak_opt_coqide_buffer ClientClose .* %{ coqide-stop }

      hook -group coqide buffer=$kak_opt_coqide_buffer InsertChar .* coqide-on-text-change
      hook -group coqide buffer=$kak_opt_coqide_buffer InsertDelete .* coqide-on-text-change
    "
  }

  add-highlighter -override buffer/coqide_errors ranges coqide_error_range
  add-highlighter -override buffer/coqide_processed ranges coqide_processed_range
}

define-command -docstring "
  Update the state of the highlighter when new text is added before its tip.
  Does nothing if the text inserted is after the tip.
" -hidden -params 0 coqide-on-text-change %{
  evaluate-commands %sh{
    IFS=" .,|" read -r _ _ _ eline ecol _ <<< "$kak_opt_coqide_processed_range"
    eline=${eline:-1}
    ecol=${ecol:-1}

    IFS=" .,|" read -r _ _ _ errline errcol _ <<< "$kak_opt_coqide_error_range"
    errline=${errline:-1}
    errcol=${errcol:-1}

    first_selection=$(sort -g <<< "${kak_selections_desc// /$'\n'}" | uniq | head -1)
    IFS=".,|" read -r _ _ sline scol _ <<< "$first_selection"
    sline=${sline:-1}
    scol=${scol:-1}

    if [ "$sline" -lt "$eline" -o "$sline" -eq "$eline" -a "$scol" -lt "$ecol" ]; then
      # NOTE: `-a` has a bigger precedence than `-o`, so the test above is really
      #       `$sline < $eline || ($sline == $eline && $scol < $ecol)`

      echo "coqide-invalidate-state $sline $scol"
    fi
    if [ "$sline" -lt "$errline" -o "$sline" -eq "$errline" -a "$scol" -lt "$errcol" ]; then
      # NOTE: `-a` has a bigger precedence than `-o`, so the test above is really
      #       `$sline < $errline || ($sline == $errline && $scol < $errcol)`

      echo "coqide-invalidate-error"
    fi
  }
}

define-command -docstring "
  `coqide-invalidate-state <line> <col>`: Invalidates the current processed state at least until the first selection (indicated by the 2 parameters).
  - `<line>`: the line number where the first selection lies
  - `<col>`: the column number (character offset) where the first selection lies

  Warning: this command must /NOT/ be called by hand at all, unless trying to debug its behavior.
  It makes strong assumptions about the current state (basically that the first selection in the buffer is
  before the end of the current tip).
" -hidden -params 2 coqide-invalidate-state %{
  evaluate-commands -draft -save-regs "/" %{
    try %{ execute-keys "$[ $kak_main_reg_hash -eq 1 ]<ret>" }
    coqide-send-to-process "rewind-to %arg{1} %arg{2}"
  }
}

define-command -docstring "
  
" -hidden -params 0 coqide-invalidate-error %{
  coqide-send-to-process "ignore-error"
}

define-command -docstring "
  Move to the end of the next Coq statement if cursor does not point to a `.`, else send until cursor.
" -params 0 coqide-move-to %{
  try %{
    declare-option -hidden int coqide_move_line0
    declare-option -hidden int coqide_move_col0
    declare-option -hidden int coqide_move_line1
    declare-option -hidden int coqide_move_col1
    declare-option -hidden str coqide_move_tmp_file
    declare-option -hidden bool coqide_move_invalidated true
  }
  set-option buffer coqide_move_tmp_file "%opt{coqide_pipe}/tmp"
  
  evaluate-commands -draft %{
    try %{
      execute-keys "$[ $kak_main_reg_hash -eq 1 ]<ret>"
    }
    # jump to the first character of the selection:
    # (ensure forward direction (cursor at end); flip direction (cursor at beginning); reduce selection to cursor)
    execute-keys "<a-:><a-;>;<ret>"

    evaluate-commands %sh{
      if ! [ -f "$kak_opt_coqide_move_tmp_file" ]; then
        touch "$kak_opt_coqide_move_tmp_file"
      else
        printf '' >"$kak_opt_coqide_move_tmp_file"
      fi
      
      IFS=" .,|" read -r _ _ _ line0 col0 _ <<< "$kak_opt_coqide_processed_range"
      line0=${line0:-1}
      col0=${col0:-1}

      echo "set-option buffer coqide_move_line0 '$line0'"
      echo "set-option buffer coqide_move_col0 '$col0'"

      first_selection=$(sort -g <<< "${kak_selections_desc// /$'\n'}" | uniq | head -1)
      IFS=".,|" read -r _ _ line1 col1 _ <<< "$first_selection"
      line1=${line1:-1}
      col1=${col1:-1}

      echo "set-option buffer coqide_move_line1 '$line1'"
      echo "set-option buffer coqide_move_col1 '$col1'"

      if ! [ $line0 -eq 1 -a $col0 -eq 1 ]; then
        echo "set-option -add buffer coqide_move_col0 1"
      fi
    }
    evaluate-commands %sh{
      if [ $kak_opt_coqide_move_line1 -lt $kak_opt_coqide_move_line0 \
         -o $kak_opt_coqide_move_line1 -eq $kak_opt_coqide_move_line0 -a $kak_opt_coqide_move_col1 -lt $kak_opt_coqide_move_col0 ]; then
        # NOTE: `-a` has a bigger precedence than `-o`, so the test above is really
        #       `$sline < $eline || ($sline == $eline && $scol < $ecol)`
        #
        # This test checks whether the first cursor is before the end of the processed range.

        echo "coqide-invalidate-state $kak_opt_coqide_move_line1 $kak_opt_coqide_move_col1"
      else
        # Go to the last processed line and column and select everything in the buffer
        # until the end
        keys="${kak_opt_coqide_move_line0}ggh"
        if ! [ $kak_opt_coqide_move_line0 -eq 1 -a $kak_opt_coqide_move_col0 -eq 1 ]; then
          keys="${keys}${kak_opt_coqide_move_col0}l"
        fi
        keys="${keys}Ge<a-|>cat<space><gt>$kak_opt_coqide_move_tmp_file<ret>"
        echo "execute-keys '$keys'"
        echo "set-option buffer coqide_move_invalidated false"
      fi
    }
    evaluate-commands %sh{
      if [ "$kak_opt_coqide_move_invalidated" == "true" ]; then
        exit
      fi
  
      all_ranges=$(python3 "$kak_opt_coqide_source"/../parse_coq.py \
        "$kak_opt_coqide_move_line0" "$kak_opt_coqide_move_col0" "to" "$kak_opt_coqide_move_line1" "$kak_opt_coqide_move_col1" <"$kak_opt_coqide_move_tmp_file")
      printf '' >"$kak_opt_coqide_move_tmp_file"

      for span in $(printf "%s\n" $all_ranges); do      
        IFS=".," read -r bline bcol eline ecol _ <<< "$span"
        bline=${bline:-1}
        bcol=${bcol:-1}
        eline=${eline:-1}
        ecol=${ecol:-1}

        position="$(printf "%d.%d,%d.%d," "$bline" "$bcol" "$eline" "$ecol")"
        
        # Get all the code between $bline:$bcol and $eline:$col
        keys="${bline}ggh"
        if [ $bcol -ne 1 ]; then
          keys="$keys$((bcol - 1))l"
        fi
        if [ $bline -eq $eline ]; then
          keys="$keys$((ecol - bcol))L"
        else
          if [ $eline -gt ${kak_buf_line_count:-1} ]; then
            keys="${keys}Ge"
          else
            keys="$keys$((eline - bline))JGh$((ecol - 1))L"
          fi
        fi
        echo "execute-keys %§$keys<a-|>printf<space>\"<percent>s\"<space>\"$position\\\"\$(sed<space>'s/\"/\\\\\"/g')\\\"<space>\"<space><gt><gt>$kak_opt_coqide_move_tmp_file<ret>§"
      done
    }
    evaluate-commands %sh{
      if [ "$kak_opt_coqide_move_invalidated" == "true" ]; then
        exit
      fi      

      all_ranges="$(cat "$kak_opt_coqide_move_tmp_file")"
      echo "coqide-send-to-process %§move-to $(sed 's/[[:space:]]*$//' <<< "$all_ranges")§"

      rm "$kak_opt_coqide_move_tmp_file"
    }

    unset-option buffer coqide_move_col0
    unset-option buffer coqide_move_col1
    unset-option buffer coqide_move_invalidated
    unset-option buffer coqide_move_line0
    unset-option buffer coqide_move_line1
    unset-option buffer coqide_move_tmp_file
  }
}

define-command -docstring "
  Get hints for the current proof from the underlyign coqidetop process/
" -params 0 coqide-hints %{
  coqide-send-to-process 'hints'
}

define-command -docstring "
  Cancel the lastly processed Coq statement.  
" -params 0 coqide-previous %{
  coqide-send-to-process 'previous'
}

define-command -docstring "
  Send the next Coq statement.
" -params 0 coqide-next %{
  try %{
    declare-option -hidden int coqide_next_line0
    declare-option -hidden int coqide_next_col0
    declare-option -hidden str coqide_next_span
    declare-option -hidden str coqide_next_tmp_file 
  }
  set-option buffer coqide_next_tmp_file "%opt{coqide_pipe}/tmp"

  evaluate-commands -draft %sh{
    if ! [ -f "$kak_opt_coqide_next_tmp_file" ]; then
      touch "$kak_opt_coqide_next_tmp_file"
    else
      printf '' >"$kak_opt_coqide_next_tmp_file"
    fi
    
    # <timestamp> <begin_line>.<begin_column>,<end_line>.<end_column>|<face>
    IFS=".,| " read -r _ _ _ line0 col0 _ <<< "$kak_opt_coqide_processed_range"
    line0=${line0:-1}
    col0=${col0:-1}

    keys="${line0}ggh"
    if ! [ $line0 -eq 1 -a $col0 -eq 1 ]; then
      keys="${keys}${col0}l"
    fi
    keys="${keys}Ge<a-|>cat<space><gt>$kak_opt_coqide_next_tmp_file<ret>"
    echo "execute-keys '$keys'"
    echo "set-option buffer coqide_next_line0 '$line0'"
    echo "set-option buffer coqide_next_col0 '$col0'"

    if ! [ $line0 -eq 1 -a $col0 -eq 1 ]; then
      echo "set-option -add buffer coqide_next_col0 1"
    fi
  }
  evaluate-commands -draft %sh{
    next_range=$(python3 "$kak_opt_coqide_source"/../parse_coq.py "$kak_opt_coqide_next_line0" "$kak_opt_coqide_next_col0" "next" <"$kak_opt_coqide_next_tmp_file")
    printf '' >"$kak_opt_coqide_next_tmp_file"

    IFS="., " read -r bline bcol eline ecol _ <<< "$next_range"
    bline=${bline:-1}
    bcol=${bcol:-1}
    eline=${eline:-1}
    ecol=${ecol:-1}
    # Get all the code between $bline:$bcol and $eline:$col
    keys="${bline}ggh"
    if [ $bcol -ne 1 ]; then
      keys="$keys$((bcol - 1))l"
    fi
    if [ $bline -eq $eline ]; then
      keys="$keys$((ecol - bcol))L"
    else
      if [ $eline -gt ${kak_buf_line_count:-1} ]; then
        keys="${keys}Ge"
      else
        keys="$keys$((eline - bline))JGh$((ecol - 1))L"
      fi
    fi
    echo "execute-keys %§$keys<a-|>sed<space>'s/\"/\\\\\"/g'<space><gt>$kak_opt_coqide_next_tmp_file<ret>§"
    echo "set-option buffer coqide_next_span '$next_range'"
  }
  evaluate-commands %sh{
    IFS="., " read -r bline bcol eline ecol _ <<< "$kak_opt_coqide_next_span"
    bline=${bline:-1}
    bcol=${bcol:-1}
    eline=${eline:-1}
    ecol=${ecol:-1}
        
    code="$(cat "$kak_opt_coqide_next_tmp_file")"
    echo "coqide-send-to-process %§next $bline.$bcol,$eline.$ecol,\"$code\"§"

    rm "$kak_opt_coqide_next_tmp_file"
  }

  unset-option buffer coqide_next_line0
  unset-option buffer coqide_next_col0
  unset-option buffer coqide_next_span
  unset-option buffer coqide_next_tmp_file
}

define-command -docstring "
  Send a query directly to the underlying `coqidetop` process, in a disposable context.
" -params 0 coqide-query %{
  prompt 'Query:' %{
    coqide-send-to-process %sh{ echo "query \"$(sed 's/"/\\"/g' <<< "$kak_text")\"" }
  }
}

define-command -docstring "
  `coqide-send-to-process <cmd>`: sends a command to the coqide-kak process.
" -hidden -params 1 coqide-send-to-process %{
  evaluate-commands %sh{
    if [ -z "$kak_opt_coqide_pid" ]; then
      echo "fail 'coqide: not started in current buffer'"
    fi
  }

  nop %sh{
    >&2 echo "Sending %§$1§"

    exec 4<>"$kak_opt_coqide_pipe/cmd"
    echo "$1" >>"$kak_opt_coqide_pipe/cmd"
  }
}

define-command -docstring "
  Creates a named pipe, and starts listening (with `socat`) on a Unix socket.
" -hidden -params 0 coqide-populate-fd4 %{
  try %{
    set-option buffer coqide_socat_pid %sh{
      # Open a new named pipe to transfer data to `socat`
      if [ -f "$kak_opt_coqide_pipe/cmd" ]; then
        rm "$kak_opt_coqide_pipe/cmd" &>/dev/null

        # Close the file descriptor 4 which was linked to the pipe in reading & writing
        exec 4>&-
        exec 4<&-
      fi
      mkfifo "$kak_opt_coqide_pipe/cmd"
      exec 4<>"$kak_opt_coqide_pipe/cmd"
      
      socat -u PIPE:"$kak_opt_coqide_pipe/cmd" UNIX-CONNECT:"$kak_opt_coqide_pipe/cmd.sock" &>"$kak_opt_coqide_pipe/log" </dev/null &
      echo "$!"
    }
  }
}

define-command -docstring "
  Dump the log in a specific buffer for user examination.
" -params 0 coqide-dump-log %{
  edit! -debug -readonly -fifo "%opt{coqide_pipe}/log" -scroll "%opt{coqide_log_buffer}"
}

define-command -docstring "
  Stop `coqide-kak`, giving up on the last state.

  Also deletes the control pipe.
" -params 0 coqide-stop %{
  evaluate-commands %sh{
    if [ -z "$kak_opt_coqide_pid" ]; then
      echo 'fail "coqide: not started in current buffer"'
    fi
  }
  
  remove-hooks buffer coqide

  # NOTE: do not put all those lines in the same `try` block, as we want to be able
  #       to individually delete each one 
  try %{ delete-buffer! "%opt{coqide_log_buffer}" }
  try %{ delete-buffer! "%opt{coqide_goal_buffer}" }
  try %{ delete-buffer! "%opt{coqide_result_buffer}" }

  try %{
    evaluate-commands %sh{
      echo "coqide-send-to-process 'quit'"
      
      if ! kill -INT "$kak_opt_coqide_pid" &>/dev/null; then
        echo 'fail "coqide: process %opt{coqide_pid} already dead"'
      fi
      rm -r "$kak_opt_coqide_pipe"

      # Close the file descriptor 4
      exec 4>&-
      exec 4<&-

      # Kill our `socat` process
      kill -KILL "$kak_opt_coqide_socat_pid" &>/dev/null || true
    }
  }

  remove-highlighter buffer/coqide_processed
  remove-highlighter buffer/coqide_errors

  unset-option buffer coqide_log_buffer
  unset-option buffer coqide_goal_buffer
  unset-option buffer coqide_result_buffer
  unset-option buffer coqide_socat_pid
  unset-option buffer coqide_pid 
  unset-option buffer coqide_processed_range
}
        
define-command -docstring "
  Move the main cursor to the tip of the processed range.
" -params 0 coqide-goto-tip %{
  execute-keys %sh{
    IFS=' .,|' read -r _ _ _ eline ecol _ <<< "$kak_opt_coqide_processed_range"
    eline=${eline:-1}
    ecol=$((${ecol:-1} - 1))

    echo "${eline}ggh${ecol}l"
  }
}
