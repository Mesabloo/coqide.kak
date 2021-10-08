# Interface:
# - coqide_command
# - coqide_processed
# - coqide-start
# - coqide-stop
# - coqide-dump-log
# - coqide-next
# - coqide-previous
# - coqide-move-to

declare-option -docstring "
  Command to launch `coqide-kak`.
" str coqide_command "coqide-kak"

set-face global coqide_processed default,black

####################################################################################################

declare-option -docstring "
  The PID of the coqide-kak process used to interact with kakoune.
" -hidden str coqide_pid

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
  The name of the CoqIDE logging buffer.
" -hidden str coqide_log_buffer "*coqide-log*"

define-command -docstring "
  Start `coqide-kak` for the current buffer.  
" -params 0 coqide-start %{
  set-option buffer coqide_buffer %val{buffile}
  set-option buffer coqide_processed_range %val{timestamp} '1.1,1.1|coqide_processed'
  
  set-option buffer coqide_pipe %sh{
    filename=$(sed 's|/|_|g' <<< "$kak_opt_coqide_buffer")
    echo "/tmp/coqide-${kak_session}-pipe-${filename}"
  }

  nop %sh{
    mkdir -p "$kak_opt_coqide_pipe"
    
  #   mkfifo "$kak_opt_coqide_pipe/goal"
  #   mkfifo "$kak_opt_coqide_pipe/result"
  #   mkfifo "$kak_opt_coqide_pipe/log"
  #   mkfifo "$kak_opt_coqide_pipe/cmd"

  #   exec 3<> "$kak_opt_coqide_pipe/goal"
  #   exec 3<> "$kak_opt_coqide_pipe/result"
  #   exec 3<> "$kak_opt_coqide_pipe/log"
  #   exec 3<> "$kak_opt_coqide_pipe/cmd"
  }

  evaluate-commands %sh{
    if ! type "$kak_opt_coqide_command" &>/dev/null; then
      echo "fail 'coqide: Unknown command \"$kak_opt_coqide_command\"'"
      exit 1
    fi
  }
  
  set-option buffer coqide_pid %sh{
    $kak_opt_coqide_command "$kak_session" "$kak_opt_coqide_buffer" "$kak_opt_coqide_pipe" </dev/null &>/dev/null &
    echo "$!"
  }

  evaluate-commands %sh{
    echo "
      hook -once -group coqide buffer=$kak_opt_coqide_buffer BufClose .* %{
        coqide-stop
      }

      hook -group coqide buffer=$kak_opt_coqide_buffer InsertChar .* coqide-on-text-change
      hook -group coqide buffer=$kak_opt_coqide_buffer InsertDelete .* coqide-on-text-change
    "
  }

  add-highlighter -override buffer/coqide_processed ranges coqide_processed_range

  coqide-send-to-process %{init}
}

define-command -docstring "
  Update the state of the highlighter when new text is added before its tip.
  Does nothing if the text inserted is after the tip.
" -hidden -params 0 coqide-on-text-change %{
  evaluate-commands -draft -save-regs '/' %sh{
    IFS=" .,|" read -r _ _ _ eline ecol _ <<< "$kak_opt_coqide_processed_range"
    eline=${eline:-0}
    ecol=${ecol:-0}

    all_selections="$kak_selections_desc"
    first_selection=$(sort -g <<< "${all_selections// /$'\n'}" | uniq | head -1)
    IFS=".,|" read -r _ _ sline scol <<< "$first_selection"

    if [ "$sline" -lt "$eline" -o "$sline" -eq "$eline" -a "$scol" -lt "$ecol" ]; then
      # NOTE: `-a` has a bigger precedence than `-o`, so the test above is really
      #       `$sline < $eline || ($sline == $eline && $scol < $ecol)`

      # jump to the first `.` before the first selection, and only keep this selection
      echo '
        try %{
          execute-keys "$[ $kak_main_reg_hash -eq 1 ]<ret>"
        }'
      echo 'set-register slash "\."'
      echo 'execute-keys "<a-N><space>"'
      echo 'coqide-move-to'
    fi
  }
}

define-command -docstring "
  Move to the end of the next Coq statement if cursor does not point to a `.`, else send until cursor.
" -params 0 coqide-move-to %{
  evaluate-commands -draft -save-regs '/' %sh{
    echo '
      try %{
        execute-keys "$[ $kak_main_reg_hash -eq 1 ]<ret>"
      }'
    # jump to the first character of the selection:
    # (ensure forward direction (cursor at end); flip direction (cursor at beginning); reduce selection to cursor)
    echo 'execute-keys "<a-:><a-;>;<ret>"'

    if [ "$kak_selection" != "." ]; then
      # if our first cursor is not on a `.`, then go to the next one
      echo 'set-register slash "\."'
      echo 'execute-keys "N<space>"'
    fi

    IFS=".," read -r line col _ _ <<< "$selection_desc"
    echo "coqide-send-to-process %{goto '$line' '$col'}"
  }
}


define-command -docstring "
  Cancel the lastly processed Coq statement.  
" -params 0 coqide-previous %{
  # TODO
}

define-command -docstring "
  Send the next Coq statement.
" -params 0 coqide-next %{
  # TODO
}

define-command -docstring "
  Sends a command to the coqide-kak process.
" -hidden -params 1 coqide-send-to-process %{
  nop %sh{
    echo "$1" >> "$kak_opt_coqide_pipe/cmd"
    kill -USR1 "$kak_opt_coqide_pid"
  }
}

define-command -docstring "
  Dump the log in a specific buffer for user examination.
" -params 0 coqide-dump-log %{
  try %{
    evaluate-commands -draft %{
      buffer %opt{coqide_log_buffer}
      delete-buffer! %opt{coqide_log_buffer}
    }
  }
  
  edit -debug -fifo "%opt{coqide_pipe}/log" -scroll %opt{coqide_log_buffer}
}

define-command -docstring "
  Stop `coqide-kak`, giving up on the last state.

  Also deletes the control pipe.
" -params 0 coqide-stop %{
  remove-hooks buffer coqide
  nop %sh{ exec 3>&- }

  nop %sh{
    kill -INT "$kak_opt_coqide_pid"
    rm -r "$kak_opt_coqide_pipe"
  }

  remove-highlighter buffer/coqide_processed

  unset-option buffer coqide_pid 
  unset-option buffer coqide_processed_range
}
