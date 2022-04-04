declare-option -docstring '
  The path to this file, when sourced, so that we can call various utility scripts.
' -hidden str coqide_source %sh{
  printf "%s" "${kak_source%/rc/*}"
}



provide-module coqide %(


declare-option -docstring '
  The range containing pieces of code which are yet to be processed by CoqIDE.

  The face can be customised by changing the face `coqide_to_be_processed_face`.
' -hidden range-specs coqide_to_be_processed_range # %val{timestamp}
declare-option -docstring '
  The range spanning all across every piece of processed code.

  The face can be customised by changing the face `coqide_processed_face`.
' -hidden range-specs coqide_processed_range # %val{timestamp}
declare-option -docstring '
  The range spanning the last error encountered.

  The face can be customised by changing the face `coqide_error_face`.
' -hidden range-specs coqide_error_range # %val{timestamp}
declare-option -docstring '
  The range spanning all code containing admitted proofs.

  The face can be customised by changing the face `coqide_admitted_face`.
' -hidden range-specs coqide_admitted_range # %val{timestamp}
declare-option -docstring '
  The highlighter for the goal buffer, because it is better with colors.
' -hidden range-specs coqide_goal_highlight # %val{timestamp}
declare-option -docstring '
  The highlighter for the result buffer, because it is also better with colors.
' -hidden range-specs coqide_result_highlight # %val{timestamp}

declare-option -docstring '
  The last timestamp the buffer was checked for change.
' -hidden int coqide_last_checked_timestamp # %val{timestamp}

declare-option -docstring '
  The PID of the CoqIDE daemon.
' -hidden str coqide_pid
declare-option -docstring '
  The PID of the socat communication process.
' -hidden str coqide_socat_pid

declare-option -docstring '
  The format used to set the name of the goal buffer.

  It must take one parameter:
  1. "%s", the name of the buffer CoqIDE has been started for, extracted from `%opt{coqide_buffer}`
  2. "%d", the PID of the CoqIDE daemon (retrieved with `%opt{coqide_pid}`)
' -hidden str coqide_buffer_goal_format 'goal-%d'
declare-option -docstring '
  The format used to set the name of the result buffer.

  It must take one parameter:
  1. "%s", the name of the buffer CoqIDE has been started for, extracted from `%opt{coqide_buffer}`
  2. "%d", the PID of the CoqIDE daemon (retrieved with `%opt{coqide_pid}`)
' -hidden str coqide_buffer_result_format 'result-%d'
declare-option -docstring '
  The format used to set the name of the log buffer.

  It must take one parameter:
  1. "%s", the name of the buffer CoqIDE has been started for, extracted from `%opt{coqide_buffer}`
  2. "%d", the PID of the CoqIDE daemon (retrieved with `%opt{coqide_pid}`)
' -hidden str coqide_buffer_log_format 'log-%d'
declare-option -docstring '
  The name of the buffer which the CoqIDE process has been started for.
' -hidden str coqide_buffer
declare-option -docstring '
  The name of the result buffer, determined from the format `%opt{coqide_buffer_result_format}`.
' -hidden str coqide_result_buffer 
declare-option -docstring '
  The name of the goal buffer, derived from the format specified in `%opt{coqide_buffer_goal_format}`.
' -hidden str coqide_goal_buffer
declare-option -docstring '
  The name of the log buffer, set using the format in `%opt{coqide_buffer_log_format}`.
' -hidden str coqide_log_buffer 

declare-option -docstring '
  The command use to start the CoqIDE daemon.

  This can be customised to suite multiple systems, and defaults to `coqide-daemon`, which
  assumes that the executable is in your PATH.
' str coqide_command "coqide-daemon"

declare-option -docstring '
  The directory containing all temporary files such as control pipes.
' -hidden str coqide_pipe_dir 
declare-option -docstring '
  The path to the file serving as input control (for kakoune commands to be sent to the daemon).
' -hidden str coqide_fifo_input 
declare-option -docstring '
  The file where all logs are written to.
' -hidden str coqide_log_output
declare-option -docstring '
  
' -hidden str coqide_socket_input


# The face used to highlight code which is to be processed by CoqIDE.
# Defaults to `default,magenta`.
set-face global coqide_to_be_processed_face default,magenta
# The face higlighting processed code.
# Defaults to `default,green`.
set-face global coqide_processed_face default,green
# The face which indicates an error has occurred somewhere.
# Defaults to `default,red`.
set-face global coqide_error_face default,red
# The face to highlight ranges with admitted axioms.
# Defaults to `default,yellow`.
set-face global coqide_admitted_face default,yellow

# --- Faces to highlight the goal/result buffers ---
set-face global coqide_keyword @keyword
set-face global coqide_evar @variable
set-face global coqide_type @type
set-face global coqide_notation @operator
set-face global coqide_variable @variable
set-face global coqide_reference @variable
set-face global coqide_path @module
set-face global coqide_warning yellow+b
set-face global coqide_error red+b




define-command -docstring '
  Start CoqIDE in the current buffer, if it isn''t already started there.
' -params 0 coqide-start %{
  evaluate-commands %sh{
    if [ -n "$kak_opt_coqide_pid" ]; then
      echo "fail 'coqide: already started in the current buffer'"
      exit
    fi
  }

  set-option buffer coqide_buffer %val{bufname}
  evaluate-commands %sh{
    filename="${kak_opt_coqide_buffer//[^A-Za-z0-9._-]/_}"
    tmp_dir="$(mktemp -d)"

    mkdir -p "$tmp_dir" &>/dev/null

    echo "set-option buffer coqide_pipe_dir '$tmp_dir/'"
    echo "set-option buffer coqide_fifo_input '$tmp_dir/input'"
    echo "set-option buffer coqide_log_output '$tmp_dir/log'"
    echo "set-option buffer coqide_socket_input '$tmp_dir/input.sock'"
  }

  evaluate-commands %sh{
    if ! type "$kak_opt_coqide_command" &>/dev/null; then
      echo "fail 'coqide: cannot execute \"$kak_opt_coqide_command\": no such executable file'"
      exit
    fi
  }

  set-option buffer coqide_pid %sh{    
    env RUST_BACKTRACE=1 "$kak_opt_coqide_command" "$kak_session" "$kak_opt_coqide_buffer" "$kak_opt_coqide_pipe_dir" "$kak_opt_coqide_socket_input" \
    </dev/null &>"$kak_opt_coqide_log_output" &

    echo "$!"
  }

  evaluate-commands %sh{
    echo "
      hook -once -group coqide buffer=$kak_opt_coqide_buffer BufClose .* coqide-stop
      hook -once -group coqide buffer=$kak_opt_coqide_buffer ClientClose .* coqide-stop
      hook -once -group coqide buffer=$kak_opt_coqide_buffer KakEnd .* coqide-stop
      
      hook -group coqide buffer=$kak_opt_coqide_buffer InsertChar .* coqide-on-text-change
      hook -group coqide buffer=$kak_opt_coqide_buffer InsertDelete .* coqide-on-text-change
      hook -group coqide buffer=$kak_opt_coqide_buffer NormalIdle .* coqide-on-idle-text-change
      hook -group coqide buffer=$kak_opt_coqide_buffer InsertIdle .* coqide-on-idle-text-change
    " # These last two hooks unfortunately do not take care of
      # text editing in normal mode (e.g. when cutting text).
      #
      # It would be great to have a hook for every buffer modification.
  }

  set-option buffer coqide_to_be_processed_range %val{timestamp}
  set-option buffer coqide_processed_range %val{timestamp}
  set-option buffer coqide_error_range %val{timestamp}
  set-option buffer coqide_admitted_range %val{timestamp}

  set-option buffer coqide_last_checked_timestamp %val{timestamp}

  add-highlighter -override buffer/coqide_to_be_processed ranges coqide_to_be_processed_range
  add-highlighter -override buffer/coqide_processed ranges coqide_processed_range
  add-highlighter -override buffer/coqide_error ranges coqide_error_range
  add-highlighter -override buffer/coqide_admitted ranges coqide_admitted_range
}
define-command -docstring '
  Initialize the extension completely.
' -hidden -params 0 coqide-init %{
  echo -debug "coqide: initializing extension..."
  
  coqide-create-buffers

  set-option buffer coqide_socat_pid %sh{
    mkfifo "$kak_opt_coqide_fifo_input" &>/dev/null
    exec 3<>"$kak_opt_coqide_fifo_input"
    socat -u PIPE:"$kak_opt_coqide_fifo_input" UNIX-CONNECT:"$kak_opt_coqide_socket_input" &>"$kak_opt_coqide_log_output" </dev/null &
    echo "$!"
  }
  
  coqide-send-command 'init'
}
define-command -docstring '
  Create additional buffers for goal/result visualisation.
' -hidden -params 0 coqide-create-buffers %{
  set-option buffer coqide_goal_buffer %sh{
    printf "$kak_opt_coqide_buffer_goal_format" "$kak_opt_coqide_pid"
  }
  set-option buffer coqide_result_buffer %sh{
    printf "$kak_opt_coqide_buffer_result_format" "$kak_opt_coqide_pid"
  }
  
  evaluate-commands -draft %{
    edit! -scratch "%opt{coqide_goal_buffer}"
    add-highlighter buffer/coqide_goal ranges coqide_goal_highlight
  }
  evaluate-commands -draft %{
    edit! -scratch "%opt{coqide_result_buffer}"
    add-highlighter buffer/coqide_result ranges coqide_result_highlight
  }
}

define-command -docstring '
  Send a command to the CoqIDE daemon.
' -hidden -params 1 coqide-send-command %{
  evaluate-commands %sh{
    if [ -z "$kak_opt_coqide_pid" ]; then
      echo "fail 'coqide: not started in the current buffer'"
      exit
    fi

    echo "echo -debug %§coqide: sending '${1//§//§§}' to daemon...§"

    echo "$1" >>"$kak_opt_coqide_fifo_input"
  }
}

define-command -docstring '
  Send the next Coq command to the daemon and update the "to be processed" range.
  Also jump to the new tip.
' -params 0 coqide-next %{
  evaluate-commands -draft -save-regs 'a' %{
    coqide-to-be-processed-range
    execute-keys %sh{
      IFS=' .,|' read -r begin_line begin_column end_line end_column _ <<< "$kak_reg_a" 
      begin_line=${begin_line:-1}
      begin_column=${begin_column:-1}
      end_line=${end_line:-1}
      end_column=${end_column:-1}

      keys="${end_line}ggh"
      if [ "$end_column" -gt 1 ]; then
        keys="${keys}${end_column}l"
      fi
      keys="${keys}Ge<a-;>" 
      
      echo "$keys"
    }
    evaluate-commands %sh{
      case $kak_selection in
        (*[![:space:]]*)
            IFS=$'\n'
            set -- $(python3 $kak_opt_coqide_source/parse_coq.py $kak_cursor_line $kak_cursor_column next <<< "$kak_selection")
            while [ $# -gt 0 ]; do
              range="${1%% *}"
              code=$(sed -e "s/\\\\n/\n/g" <<< "${1#* }")

              echo "echo -debug %§next $range $code§"

              echo "coqide-push-to-be-processed '$range'"
              echo "coqide-send-command %§next $range $code§"
              echo "coqide-send-command %§show-goals $range§"
              echo "coqide-send-command 'status'"

              shift
            done
          ;;
        (*) exit
          ;;
      esac
    }
  }
  coqide-goto-tip
}

define-command -docstring '
  Move the main cursor to the tip of the area to be processed.
' -params 0 coqide-goto-tip %{
  echo -debug "coqide: moving cursor to tip"
  
  execute-keys %sh{
    range=`(tr ' ' '\n' | sed -e '$!d' | tr '\n' ' ') <<< "$kak_opt_coqide_to_be_processed_range"`
    #                             ^^^ remove all but the last line
    IFS=' |.,' read -r _ _ eline ecol _ <<< "$range"
    eline=${eline:-1}
    ecol=${ecol:-1}

    echo "${eline}ggh"
    if [ "$ecol" -gt 1 ]; then
      echo "$((ecol - 1))l"
    fi
  }
}

define-command -docstring '
  Open the log buffer and show everything the CoqIDE daemon has to tell us.
' -params 0 coqide-dump-log %{
  set-option buffer coqide_log_buffer %sh{
    printf "$kak_opt_coqide_log_format" "$kak_opt_coqide_pid"
  }
  
  edit! -existing -debug -readonly -fifo %opt{coqide_log_output} -scroll %opt{coqide_log_buffer}
}

define-command -docstring '
  Try to unprove the last processed statement, backtracking the processed state by one.
' -params 0 coqide-previous %{
  evaluate-commands %sh{
    IFS=' ' read -r _ r1 <<< "$kak_opt_coqide_processed_range"
    IFS=' ' read -r _ r2 <<< "$kak_opt_coqide_to_be_processed_range"
    if [ -z "$r1" -a -z "$r2" ]; then
      # if both ranges are empty, then there's nothing to backtrack, so we
      # don't do anything here.
      echo ""
    else
      echo "coqide-pop-top-to-be-processed"
      echo "coqide-send-command %§previous§"
      echo "coqide-send-command %§show-goals 1.1,1.1§"
      echo "coqide-send-command 'status'"
    fi
  }
}

define-command -docstring '
  Process code until the position of the cursor.
' -params 0 coqide-move-to %{
  evaluate-commands -draft -save-regs 'abc' %{
    set-register b %val{cursor_line}
    set-register c %val{cursor_column}

    coqide-to-be-processed-range
    execute-keys %sh{
      IFS=' .,|' read -r begin_line begin_column end_line end_column _ <<< "$kak_reg_a" 
      begin_line=${begin_line:-1}
      begin_column=${begin_column:-1}
      end_line=${end_line:-1}
      end_column=${end_column:-1}

      if [ $kak_reg_b -lt $end_line ] || [ $kak_reg_b -eq $end_line -a $kak_reg_c -lt $end_column ]; then
        echo ""
        # If the cursor is before the end of the to be processed range, we are trying to backtrack
        # to a given location.
        # Therefore, don't select anything from the buffer.
      else
        keys="${end_line}ggh"
        if [ "$end_column" -gt 1 ]; then
          keys="${keys}${end_column}l"
        fi
        keys="${keys}Ge<a-;>" 
        
        echo "$keys"
      fi
    }
    evaluate-commands %sh{
      IFS=' .,|' read -r begin_line begin_column end_line end_column _ <<< "$kak_reg_a" 
      begin_line=${begin_line:-1}
      begin_column=${begin_column:-1}
      end_line=${end_line:-1}
      end_column=${end_column:-1}
      
      if [ $kak_reg_b -lt $end_line ] || [ $kak_reg_b -eq $end_line -a $kak_reg_c -lt $end_column ]; then
        echo "coqide-invalidate-state $kak_reg_b $kak_reg_c"
        # And simply invalidate the state up until the saved line/column positions
      else 
        case $kak_selection in
          (*[![:space:]]*)
            last_range=
            IFS=$'\n'
            set -- $(python3 $kak_opt_coqide_source/parse_coq.py $kak_cursor_line $kak_cursor_column to $kak_reg_b $kak_reg_c <<< "$kak_selection")
            while [ $# -gt 0 ]; do
              range="${1%% *}"
              code=$(sed -e "s/\\\\n/\n/g" <<< "${1#* }")

              echo "coqide-push-to-be-processed '$range'"
              echo "coqide-send-command %§next $range $code§"
              last_range="$range"

              shift
            done
            echo "coqide-send-command %§show-goals ${last_range:-'1.1,1.1'}§"
            echo "coqide-send-command 'status'"
            ;;
          (*) exit
            ;;
        esac
      fi
    }
  }
  coqide-goto-tip
}

#############################################################################

define-command -docstring '
  Check if text changed before tip on idle.
' -hidden -params 0 coqide-on-idle-text-change %{
  # TODO: fix this, because text (and ranges) are modified
  # before this command is triggered...
  # which means that the backend is not able to remove ranges 
  evaluate-commands %sh{
    if [ "$kak_opt_coqide_last_checked_timestamp" -ne "$kak_timestamp" ]; then
      echo "coqide-on-text-change"
    fi
  }
  set-option buffer coqide_last_checked_timestamp %val{timestamp}
}

define-command -docstring '
  Check if text has been changed before the tip.
  If this is the case, backtrack to the state before the cursor position.
' -hidden -params 0 coqide-on-text-change %{
  evaluate-commands -draft %{
    execute-keys "$[ $kak_main_reg_hash -eq 1 ]"
    evaluate-commands %sh{
  range=`(tr ' ' '\n' | sed -e '$!d' | tr '\n' ' ') <<< "$kak_opt_coqide_to_be_processed_range"`
      #                             ^^^ remove all but the last line
      IFS=' |.,' read -r _ _ eline_p ecol_p _ <<< "$range"
      eline_p=${eline_p:-1}
      ecol_p=${ecol_p:-1}
      
      IFS=' |.,' read -r _ _ _ eline_e ecol_e _ <<< "$kak_opt_coqide_error_range"
      eline_e=${eline_e:-1}
      ecol_e=${ecol_e:-1}
      
      IFS=' |.,' read -r sline scol _ _ _ <<< "$kak_selection_desc"
      sline=${sline:-1}
      scol=${scol:-1}

      if [ $sline -lt $eline_e ] || [ $sline -eq $eline_e -a $scol -lt $ecol_e ]; then
        echo "coqide-invalidate-error"
      fi
      if [ $sline -lt $eline_p ] || [ $sline -eq $eline_p -a $scol -lt $ecol_p ]; then
        echo "coqide-invalidate-state $sline $scol"
      fi 
    }
  }
}

define-command -docstring '
  Remove the last error encountered when editing before it.
' -hidden -params 0 coqide-invalidate-error %{
  coqide-send-command "ignore-error"
}

define-command -docstring '
  Return to the state indicated by the two parameters (in order: buffer line and column).
' -hidden -params 2 coqide-invalidate-state %{
  coqide-send-command "rewind-to %arg{1} %arg{2}"
  coqide-send-command "show-goals 1.1,1.1"
  coqide-send-command 'status'
}

##############################################################################

define-command -docstring '
  Refresh the content of the goal buffer.

  Arguments:
  1. `<path>`: the path to the content of the goal buffer
  2. `<ranges>`: color ranges for the highlighter
' -hidden -params 1.. coqide-refresh-goal-buffer %{
  echo -debug "coqide: refreshing goal buffer"
  evaluate-commands -buffer "%opt{coqide_goal_buffer}" %{
    execute-keys "%%|cat<space>%arg{1}<ret>"
    evaluate-commands %sh{
      if [ "$#" -eq 1 -o -z "$2" ]; then
        echo "set-option buffer coqide_goal_highlight %val{timestamp}"
      else
        shift
        echo "set-option buffer coqide_goal_highlight %val{timestamp}" "$@"
      fi
    }
  }
}
define-command -docstring '
  Refresh the content and highlighting of the result buffer.

  Arguments:
  1. `<path>`: the path to the content of the goal buffer
  2. `<ranges>`: color ranges for the highlighter
' -hidden -params 1.. coqide-refresh-result-buffer %{
  echo -debug "coqide: refreshing result buffer"
  evaluate-commands -buffer "%opt{coqide_result_buffer}" %{
    execute-keys "%%|cat<space>%arg{1}<ret>"
    evaluate-commands %sh{
      if [ "$#" -eq 1 -o -z "$2" ]; then
        echo "set-option buffer coqide_result_highlight %val{timestamp}"
      else
        shift
        echo "set-option buffer coqide_result_highlight %val{timestamp}" "$@"
      fi
    }
  }
}

###############################################################################

define-command -docstring '
  Show the status of the `coqidetop` daemon, as `Ready in <module>, proving <name>`.

  Arguments:
  1. `.`-separated module name
  2. optional proof name
' -hidden -params 2 coqide-show-status %{
  echo -debug "coqide: showing status"
  
  evaluate-commands %sh{
    msg="Ready"
    if [ ! -z "$1" ]; then
      msg="${msg} in $1"
      if [ ! -z "$2" ]; then
        msg="${msg}, proving $2"
      fi
    fi

    echo "info -markup %§{\\}${msg}§"
  }
}

define-command -docstring '
  Pop the first range present in the range for to be processed code.
' -hidden -params 0 coqide-pop-to-be-processed %{
  echo -debug "coqide: removing first range from to be processed range"
  evaluate-commands %sh{
    IFS=' |' read -r _ range _ <<< "$kak_opt_coqide_to_be_processed_range"
    echo "coqide-remove-to-be-processed $range"
  }
}

define-command -docstring '
  Pop the last range added to the to be processed code, if there is one.
' -hidden -params 0 coqide-pop-top-to-be-processed %{
  echo -debug "coqide: remove the last range from to be processed range"
  evaluate-commands %sh{
    read -r _ r1 <<< "$kak_opt_coqide_to_be_processed_range"
    if [ -z "$r1" ]; then
      # If $r1 is empty, then the range is empty so we don't need to do anything.
      echo ""
    else
      range=`(tr ' ' '\n' | sed -e '$!d' | tr '\n' ' ') <<< "$kak_opt_coqide_to_be_processed_range"`
      #                             ^^^ remove all but the last line
      IFS=' |' read -r range _ <<< "$range"
      echo "coqide-remove-to-be-processed $range"
    fi
  }
}

define-command -docstring '
  Push a new range into the range of to be processed code.
' -hidden -params 1 coqide-push-to-be-processed %{
  echo -debug "coqide: push %arg{1} to to be processed range"
  set-option -add buffer coqide_to_be_processed_range "%arg{1}|coqide_to_be_processed_face"
}

define-command -docstring '
  Remove a given range from the "to be processed" range.
' -hidden -params 1 coqide-remove-to-be-processed %{
  echo -debug "coqide: remove %arg{1} from to be processed range"
  set-option -remove buffer coqide_to_be_processed_range "%arg{1}|coqide_to_be_processed_face"
}

define-command -docstring '
  Remove a given range from the "processed" range.
' -hidden -params 1 coqide-remove-processed %{
  echo -debug "coqide: remove %arg{1} from processed range"
  set-option -remove buffer coqide_processed_range "%arg{1}|coqide_processed_face"
}

define-command -docstring '
  Add a new range to the range of processed code.
' -hidden -params 1 coqide-add-to-processed %{
  echo -debug "coqide: add %arg{1} to the processed range"
  set-option -add buffer coqide_processed_range "%arg{1}|coqide_processed_face"
}

define-command -docstring '
  Returns the complete range to be processed in register `a`.
' -hidden -params 0 coqide-to-be-processed-range %{
  set-register a %sh{
    read -r _ r1 <<< "$kak_opt_coqide_to_be_processed_range"
    if [ -z "$r1" ]; then
      # If $r1 is empty, then the range is empty so we don't need to do anything.
      echo ""
    else
      out=`(tr ' ' '\n' | sed -e '2p;$!d' | tr '\n' ' ') <<< "$kak_opt_coqide_to_be_processed_range"`
      #                           ^^^^^^ print the 2nd line, and remove all but the last line
      IFS=' .,|' read -r begin_line begin_column _ _ _ _ _ end_line end_column _ <<< "$out"
      echo "${begin_line}.${begin_column},${end_line}.${end_column}"
    fi
  }
}

define-command -docstring '
  Remove the current error range.
' -hidden -params 0 coqide-remove-error-range %{
  set-option buffer coqide_error_range %val{timestamp}
}

define-command -docstring '
  Set the error range to the given range.
' -hidden -params 1 coqide-set-error-range %{
  set-option buffer coqide_error_range %val{timestamp} "%arg{1}|coqide_error_face"
}

define-command -docstring '
  Push a range to the axiom highlighter.
' -hidden -params 1 coqide-push-axiom %{
  set-option -add buffer coqide_admitted_range "%arg{1}|coqide_admitted_face"
}

define-command -docstring '
  Remove an axiom range from the axiom highlighter.
' -hidden -params 1 coqide-remove-axiom %{
  set-option -remove buffer coqide_admitted_range "%arg{1}|coqide_admitted_face"
}

##################################################################

define-command -docstring '
  Quit CoqIDE.
' -params 0 coqide-stop %{
  evaluate-commands %sh{
    if [ -z "$kak_opt_coqide_pid" ]; then
      echo 'fail "coqide: not started in the current buffer"'
      exit
    fi
  }

  remove-hooks buffer coqide

  try %{ delete-buffer! "%opt{coqide_goal_buffer}" }
  try %{ delete-buffer! "%opt{coqide_result_buffer}" }
  try %{ delete-buffer! "%opt{coqide_log_buffer}" }

  coqide-send-command 'quit'

  remove-highlighter buffer/coqide_to_be_processed
  remove-highlighter buffer/coqide_processed
  remove-highlighter buffer/coqide_error
  remove-highlighter buffer/coqide_admitted
}
define-command -docstring '
  Purge the remaining options which must be unset after a call to `coqide-stop`.

  This command is called right after the daemon receives the `quit` command.
  Note that it will not necessarily have exited at this point, but will at some point,
  so it is not necessary to kill it here.
' -hidden -params 0 coqide-purge %{
  echo -debug "coqide: purging last remaining pieces"
  
  try %sh{
    kill -KILL "$kak_opt_coqide_socat_pid" || true
  }
  
  # Unset PIDs
  unset-option buffer coqide_pid
  unset-option buffer coqide_socat_pid
  # Delete and unset temporary files
  nop %sh{
    rm "$kak_opt_coqide_fifo_input" &>/dev/null
    rm "$kak_opt_coqide_socket_input" &>/dev/null
    rm "$kak_opt_coqide_log_output" &>/dev/null
  }
  unset-option buffer coqide_pipe_dir
  unset-option buffer coqide_fifo_input
  unset-option buffer coqide_log_output
  unset-option buffer coqide_socket_input
  unset-option buffer coqide_buffer
  
  unset-option buffer coqide_to_be_processed_range
  unset-option buffer coqide_processed_range
  unset-option buffer coqide_error_range
  unset-option buffer coqide_admitted_range
}





)
