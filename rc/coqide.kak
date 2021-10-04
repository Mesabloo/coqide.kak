declare-option -docstring "
  The PID of the coqide-kak process used to interact with kakoune.
" -hidden str coqide_kak_pid

declare-option -docstring "
  The path to the pipe used to control the `coqide-kak` process.

  This should /NOT/ be modified while the process is running, unless you want
  junk files on your system (putting them in `/tmp` should solve at least
  half of the problem).
" -hidden str coqide_kak_pipe "/tmp/coqide-%val{session}-pipe"

define-command -docstring "
  Start `coqide-kak` for the current buffer.  
" -params 0 coqide-start %{
  # TODO: handle when multiple buffers try to use `coqide-kak`
  evaluate-commands %sh{
    mkfifo $kak_opt_coqide_kak_pipe
    
    if command coqide-kak 2>/dev/null; then
      coqide-kak $kak_session $kak_buffile $kak_opt_coqide_kak_pipe 2&>1 >/dev/null &
      # NOTE: we are ignoring the output, but perhaps we would like to accumulate it
      #       to allow displaying it to the user (when requested)
      echo "set-option buffer coqide_kak_pid '$!'"
    else
      echo "echo -debug -markup 'Cannot find command \`coqide-kak\` in the PATH'"
    fi
  }

  hook -once buffer BufClose "%val{buffile}" %{
    coqide-stop
  }

  define-command -docstring "
    Stop `coqide-kak`, giving up on the last state.

    Also deletes the control pipe.
  " -params 0 coqide-stop %{
    nop %sh{
      kill -INT $kak_opt_coqide_kak_pid
      rm $kak_opt_coqide_kak_pipe
    }
  }
}
