This plugin aims at providing a functional and usable `coqidetop` wrapper for use with Kakoune.

![demo screenshot](./assets/demo1.png)

For a list of things left to do, see the end of this README.

-----------------

P.S.: [coqoune](https://github.com/guest0x0/coqoune) is the same kind of project, and was started way before this one.
However, in my experience, Coqoune has been a bit buggy at some times (for example crashing on Coq errors), and the overall integration with highlighters doesn't work that well.
I wanted to maintain a fork on my own, but most of the extension is written in some weird non-POSIX shell, which I'm not that familiar with at all.

I chose to write the “backend” in Rust, mainly to discover, and because I felt like it was more suited for this than Haskell.

## Dependencies

- coqidetop (should come with a Coq installation by default)
- [socat](https://linux.die.net/man/1/socat)
- Python 3.8+

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

## Public API

- `coqide-start` starts the daemon in the current buffer.
  Note that multiple daemons defined in multiple buffers account for multiple sessions.
  The state is also bufferized, meaning that you cannot control one daemon from another buffer than the one
  it was started for.
- `coqide-stop` stops the daemon started in the current buffer.
- `coqide-dump-log` starts a new buffer with the logs until this point.
  It is not automatically refreshed when new logs appear.
  In order to do this, you need to close the log buffer with `delete-buffer!` and relaunch `coqide-dump-log`.
- `coqide-next` identifies and processes the next Coq statement.
- `coqide-previous` removes the last processed Coq statement from the processed state.
- `coqide-query` prompts for a query to send directly to the `coqidetop` process and sends it without affecting the current state.
- `coqide-move-to` tries to process Coq statements until the main cursor.

Additional functionality:
- This plugin will aso automatically backtrack to the cursor when an insertion is detected before the end of the processed range.

## Documentation

This plugin comes with several default options, but some of them can be altered:

- `coqide_command` is the command used to launch the daemon (which is written in Rust).
  Sometimes, the executable is not in your PATH, so you may want to customize this option using `set-option global coqide_command "<path to coqide-daemon>"`.
  The default value is `coqide-daemon` therefore assumes it is in your PATH.
- **Colors:**
  - `coqide_processed` is the `face` used to highlight Coq code which has already been processed by the daemon.
    This can be customized as wanted using `set-face`, but comes with the default value `default,black`.
  - `coqide_keyword` is the `face` used to color keywords in both goal and result buffers.
    It defauls to the same face used to color `keyword`s.
  - `coqide_evar` is used to highlight specific variables in the goal and result buffers.
    Defaults to `variable` when not specified.
  - `coqide_type` is the face which colors types in the goal and results buffers.
    Defaults to the face `type` if unchanged.
  - `coqide_notation` colors operators in both goal and result buffers.
    Defaults to `operator` if left unspecified.
  - `coqide_variable` is used to highlight variable names in the goal and result buffers.
    Defaults to `variable` if unchanged.
  - `coqide_reference` ???
    Defaults to `variable` because I'm quite unsure what this is used for.
  - `coqide_path` ???
    Defaults to `module` for some reason.

## Things left to do and known bugs

The codebase is at some locations pretty ugly (e.g. when decoding XML nodes to Rust values).
However, most of it should be at last a little bit documented.

Here are some erroneous or incomplete features:
- Add ranges instead of modifying a global range
- Put different colors (e.g. orange) for ranges with given up goals (e.g. after applying the `admit` tactic).
- Highlight ranges where errors lie (e.g. in red), maybe with the possibility to underline/bold where exactly the error is
  (given by `coqidetop`, relative to the beginning of the next statement range).
- Encountering an error should backtrack the highlighted range to the last valid position and the last valid state ID.
- Only output errors once, but allow appending to the output (e.g. when doing a move to cursor).
- Trying to go past an error multiple times will make state IDs inconsistent therefore leading to a `coqidetop` error.
- Sending two `next` commands simultaneously (the second needs to be sent before the first one is processed) creates inconsistent state IDs.
  A workaround is simply to go back 2-3 states and retry processing.

  This might be easily fixable by locking everything after sending a call until a `Processed` response is received.
- The “parser” used to detect the next statement does not take in account qualified identifiers.
- ... and some other things that I did not see.

If you feel like it, feel free to improve this plugin by forking this repository and submitting your patches through pull requests.
Just try not to implement many features in the same pull request.

## Personal configuration

As I intend to use this plugin, here is how I configured it.
It spawns two new kakoune clients containing the result and goal buffers and deletes them when they get discarded.

```sh
plug "coqide.kak" do %{
  cargo build --release --locked
  cargo install --force --path . --root ~/.local
} config %{
  declare-option -hidden str coqide_close_panels

  declare-user-mode coq
  map global coq c ": enter-user-mode -lock coq<ret>" \
    -docstring "stay in the Coq user mode"
  map global coq k ": coqide-previous<ret>" \
    -docstring "unprove the last statement"
  map global coq j ": coqide-next<ret>" \
    -docstring "prove the next statement"
  map global coq <ret> ": coqide-move-to<ret>" \
    -docstring "move tip to main cursor"
  map global coq l ": coqide-dump-log<ret>" \
    -docstring "dump logs"
  map global coq q ": coqide-query<ret>" \
    -docstring "send a query to coqtop"

  # Create two additional clients to show goals and results
  hook global BufCreate \*coqide-(.*)-(goal|result)\* %{
    evaluate-commands %sh{
      client_name="coq-${kak_hook_param_capture_1}-${kak_hook_param_capture_2}"
      regex_safe="$(sed 's/\*/\\*/g' <<< "$kak_hook_param_capture_0")"

      switch_to_buffer="
        buffer $kak_hook_param_capture_0
        rename-client $client_name

        try %{
          remove-highlighter buffer/line_numbers
          remove-highlighter buffer/matching
          remove-highlighter buffer/wrap_lines
          remove-highlighter buffer/show-whitespaces
        }
      "
      ${kak_opt_termcmd} "kak -c $kak_session -e '$switch_to_buffer'" &>/dev/null </dev/null &

      echo "hook -once global BufClose '$regex_safe' %{
        evaluate-commands -client '$client_name' 'quit!'
      }"
    }
  }


  hook global WinSetOption filetype=coq %{ 
    coqide-start

    # User mode to interact with CoqIDE only in Coq files
    map buffer user c ": enter-user-mode coq<ret>" \
      -docstring "enter the Coq user mode"

    # Better looking face
    set-face global coqide_processed default,black+id

    # Commands to execute when the buffer is closed.
    # These are declared here as a string in order to have the value of `%opt{coqide_pid}`
    # (which is an internal option)
    set-option buffer coqide_close_panels "
      evaluate-commands -client coq-%opt{coqide_pid}-goal 'quit!'
      evaluate-commands -client coq-%opt{coqide_pid}-result 'quit!'
      remove-hooks coqide-%opt{coqide_pid}
    "

    # Remove the side panels when closing the buffer
    hook global -group "coqide-%opt{coqide_pid}" BufClose "%val{buffile}" %{ try %opt{coqide_close_panels} }
    hook global -group "coqide-%opt{coqide_pid}" ClientClose .* %{ try %opt{coqide_close_panels} }
  }
}
```
