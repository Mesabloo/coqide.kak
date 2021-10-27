This plugin aims at providing a functional and usable `coqidetop` wrapper for use with Kakoune.

**:warning: Disclaimer:** While this plugin somehow works sometimes, it is definitely hard to test it.
The goal was to create a correct wrapper for using Coq in Kakoune, and it has been realized.
However, working with an undocumented protocol (yes, the XML protocol of `coqidetop` is undocumented for the most part; I had to check
in other plugins such as Coqoune or VSCoq to understand some queries) as well as a heavily asynchronous codebase is very hard, and I do
not feel like I'll be really maintaining this plugin in the future.
This was more like a Proof of Concept than a real usable product (it is usable! But be aware that bugs will happen).

For a list of things left to do, see the end of this README.

-----------------

P.S.: [coqoune](https://github.com/guest0x0/coqoune) is the same kind of project, and was started way before this one.
However, in my experience, Coqoune has been a bit buggy at some times (for example crashing on Coq errors), and the overall integration with highlighters doesn't work that well.
I wanted to maintain a fork on my own, but most of the extension is written in some weird non-POSIX shell, which I'm not that familiar with at all.

I chose to write the “backend” in Rust, mainly to discover, and because I felt like it was more suited for this than Haskell.

## Dependencies

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

- `coqide_processed` is the `face` used to highlight Coq code which has already been processed by the daemon.
  This can be customized as wanted using `set-face`, but comes with the default value `default,black`.
- `coqide_command` is the command used to launch the daemon (which is written in Rust).
  Sometimes, the executable is not in your PATH, so you may want to customize this option using `set-option global coqide_command "<path to coqide-daemon>"`.
  The default value is `coqide-daemon` therefore assumes it is in your PATH.

## Things left to do and known bugs

The codebase is at some locations pretty ugly (e.g. when decoding XML nodes to Rust values).
However, most of it should be at last a little bit documented.

Here are some erroneous or incomplete features:
- Both goal and result buffers print unformatted raw text.
  This means that text like `<constr.keyword>forall</constr.keyword>` will be output, instead of a colored one.
  This makes output pretty hard to read.
- Trying to process the next statement after the end of the file yields a `coqidetop` error instead of simply be ignored.
- Trying to go past an error multiple times will make state IDs inconsistent therefore leading to a `coqidetop` error.
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
