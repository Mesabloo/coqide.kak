This plugin aims at providing a functional and usable `coqidetop` wrapper for use with Kakoune.

-----------------

P.S.: [coqoune](https://github.com/guest0x0/coqoune) is the same kind of project, and was started way before this one.
However, in my experience, Coqoune has been a bit buggy at some times (for example crashing on Coq errors), and the overall integration with highlighters doesn't work that well.
I wanted to maintain a fork on my own, but most of the extension is written in some weird non-POSIX shell, which I'm not that familiar with at all.

I chose to write the “backend” in Rust, mainly to discover, and because I felt like it was more suited for this than Haskell.

## Installation

The recommended way to install this plugin is through [plug.kak](https://github.com/andreyorst/plug.kak):
```sh
plug "mesabloo/coqide.kak" do %{
  cargo build --release --locked
  cargo install --force --path . --root ~/.local
} config %{
  # configure this plugin here
}
```

My personal configuration is:
```sh
hook global WinSetOption filetype=coq %{
  coqide-start

  # User mode to interact with CoqIDE only in Coq files
  declare-user-mode coq
  map buffer user c ": enter-user-mode coq<ret>" \
    -docstring "enter the Coq user mode"
  map buffer coq c ": enter-user-mode -lock coq<ret>" \
    -docstring "stay in the Coq user mode"
  map buffer coq k ": coqide-previous<ret>" \
    -docstring "unprove the last statement"
  map buffer coq j ": coqide-next<ret>" \
    -docstring "prove the next statement"
  map buffer coq <ret> ": coqide-move-to<ret>" \
    -docstring "move tip to main cursor"
  map buffer coq l ": coqide-dump-log<ret>" \
    -docstring "dump logs"

  # Better looking face
  set-face global coqide_processed default,default+id

  # Create two additional clients to show goals and results
  nop %sh{
    buffer_cmd() {
      buffer_name="kak_opt_${1}_buffer"
    
      echo "try %{
        buffer ${!buffer_name}
        rename-client %{coq-$1-client}
        
        remove-highlighter buffer/line_numbers
        remove-highlighter buffer/matching
        remove-highlighter buffer/wrap_lines
        remove-highlighter buffer/show-whitespaces
      }"
    }
    
    if [ -n "${kak_ot_termcmd}"]; then
      (${kak_opt_termcmd} "sh -c 'kak -c ${kak_session} -e \"$(buffer_cmd goal)\"'") &>/dev/null </dev/null &
      (${kak_opt_termcmd} "sh -c 'kak -c ${kak_session} -e \"$(buffer_cmd result)\"'") &>/dev/null </dev/null &
    fi
  }
  hook global ClientClose .* %{
    try %{
      evaluate-commands -client coq-goal-client "quit!"
      evaluate-commands -client coq-result-client "quit!"
    }
  }
}
```

## Documentation

This plugin comes with several default options, but some of them can be altered:

- `coqide_processed` is the `face` used to highlight Coq code which has already been processed by the daemon.
  This can be customized as wanted using `set-face`, but comes with the default value `default,black`.
- `coqide_command` is the command used to launch the daemon (which is written in Rust).
  Sometimes, the executable is not in your PATH, so you may want to customize this option using `set-option global coqide_command "<path to coqide-daemon>"`.
  The default value is `coqide-daemon` therefore assumes it is in your PATH.
