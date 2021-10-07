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
  Command to launch `coqide-kak`.
" str coqide_command "coqide-kak"

set-face global coqide_processed default,white+d

declare-option -docstring "
  
" -hidden range-specs coqide_processed %val{timestamp} '1.1,1.1|'

define-command -docstring "
  Start `coqide-kak` for the current buffer.  
" -params 0 coqide-start %{
  set-option buffer coqide_pipe %sh{
    filename=$(sed 's|/|_|g' <<< "$kak_buffile")
    echo "/tmp/coqide-${kak_session}-pipe-${filename}"
  }
  
  # TODO: handle when multiple buffers try to use `coqide-kak`
  set-option buffer coqide_pid %sh{
    mkfifo $kak_opt_coqide_pipe

    $kak_opt_coqide_command "$kak_session" "$kak_buffile" "$kak_opt_coqide_pipe" </dev/null &>/dev/null &
    echo "$!"
    # NOTE: we are ignoring the output, but perhaps we would like to accumulate it
    #       to allow displaying it to the user (when requested)
  }

  evaluate-commands %sh{
    echo "
      hook -once -group coqide buffer=$kak_buffile BufClose .* %{
        coqide-stop
      }
    "
  }
}

define-command -docstring "
  Sends a command to the coqide-kak process.
" -hidden -params 1 coqide-send-to-process %{
  try %sh{
    echo "$1" > "$kak_opt_coqide_pipe"
    kill -USR1 "$kak_opt_coqide_pid"
  }
}

define-command -docstring "
  Stop `coqide-kak`, giving up on the last state.

  Also deletes the control pipe.
" -params 0 coqide-stop %{
  remove-hooks buffer coqide

  nop %sh{
    kill -INT $kak_opt_coqide_pid
    rm $kak_opt_coqide_pipe
  }

  unset-option buffer coqide_pid 
  unset-option buffer coqide_processed  
}

