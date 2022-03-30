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
    "  
    #  hook -group coqide buffer=$kak_opt_coqide_buffer InsertChar .* coqide-on-text-change
    #  hook -group coqide buffer=$kak_opt_coqide_buffer InsertDelete .* coqide-on-text-change
    # " # These last two hooks unfortunately do not take care of
      # text editing in normal mode (e.g. when cutting text).
      #
      # It would be great to have a hook for every buffer modification.
  }

  set-option buffer coqide_to_be_processed_range %val{timestamp}
  set-option buffer coqide_processed_range %val{timestamp}
  set-option buffer coqide_error_range %val{timestamp}
  set-option buffer coqide_admitted_range %val{timestamp}

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

    echo "echo -debug 'coqide: sending %§$1§ to daemon...'"

    echo "$1" >>"$kak_opt_coqide_fifo_input"
  }
}

define-command -docstring '
  
' -params 0 coqide-next %{
  evaluate-commands -draft -save-regs 'a' %{
    execute-keys -draft %sh{
      IFS=' .,|' read -r _ begin_line begin_column end_line end_column _ <<< "$kak_opt_coqide_to_be_processed_range"
      begin_line=${begin_line:-1}
      begin_column=${begin_column:-1}
      end_line=${end_line:-1}
      end_column=${end_column:-1}

      keys="${end_line}ggh"
      if [ "$end_column" -gt 1 ]; then
        keys="${keys}${end_column}l"
      fi
      keys="${keys}Ge<a-;>|python3 $kak_opt_coqide_source/parse_coq.py \$kak_cursor_line \$kak_cursor_column next<ret>\"ayu"
      
      echo "$keys"
    }
    evaluate-commands %sh{
      IFS=' ' read -r range _ <<< "$kak_reg_a"
      echo "set-register a '$range'"
    }
    coqide-push-to-be-processed "%reg{a}"
    #echo -debug %opt{coqide_to_be_processed_range}
    select %reg{b}
    execute-keys -draft %{ <a-;>|sed -e 's/"/\\"/g' -e '1s/^/next $kak_reg_a "/' -e '$s/$/"/'<ret>"ayu }
    coqide-send-command "%reg{a}"
  }
  coqide-goto-tip
  echo -debug %opt{coqide_to_be_processed_range}
}

define-command -docstring '
  Move the main cursor to the tip of the area to be processed.
' -params 0 coqide-goto-tip %{
  echo -debug "coqide: moving cursor to tip"
  
  execute-keys %sh{
    IFS=' .,|' read -r _ _ _ eline ecol _ <<< "$kak_opt_coqide_to_be_processed_range"
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
  Refresh the content of the goal buffer.

  Arguments:
  1. `<path>`: the path to the content of the goal buffer
  2. `<ranges>`: color ranges for the highlighter
' -hidden -params 2 coqide-refresh-goal-buffer %{
  evaluate-commands -buffer "%opt{coqide_goal_buffer}" %{
    execute-keys "%%|cat<space>%arg{1}<ret>"
    set-option buffer coqide_goal_highlight %val{timestamp} %arg{2}
  }
}
define-command -docstring '
  Refresh the content and highlighting of the result buffer.

  Arguments:
  1. `<path>`: the path to the content of the goal buffer
  2. `<ranges>`: color ranges for the highlighter
' -hidden -params 2 coqide-refresh-result-buffer %{
  evaluate-commands -buffer "%opt{coqide_result_buffer}" %{
    execute-keys "%%|cat<space>%arg{1}<ret>"
    set-option buffer coqide_result_highlight %val{timestamp} %arg{2}
  }
}

define-command -docstring '
  Pop the first range present in the range for to be processed code.
' -hidden -params 0 coqide-pop-to-be-processed %{
  echo -debug "coqide: removing first range from to be processed range"
  evaluate-commands %sh{
    IFS=' ' read -r _ range _ <<< "$kak_opt_coqide_to_be_processed_range"
    echo "set-option -remove buffer coqide_to_be_processed_range $range"
  }
}

define-command -docstring '
  Push a new range into the range of to be processed code.
' -hidden -params 1 coqide-push-to-be-processed %{
  echo -debug "coqide: push %arg{1} to to be processed range"
  set-option -add buffer coqide_to_be_processed_range "%arg{1}|coqide_to_be_processed_face"
  #echo -debug %opt{coqide_to_be_processed_range}
}



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
