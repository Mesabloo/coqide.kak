This plugin aims at providing a functional and usable `coqidetop` wrapper for use with Kakoune.

![demo screenshot](./assets/demo4.png)

For a list of things left to do, see the end of this README.

-----------------

P.S.: [coqoune](https://github.com/guest0x0/coqoune) is the same kind of project, and was started way before this one.
However, in my experience, Coqoune has been a bit buggy at some times (for example crashing on Coq errors), and the overall integration with highlighters doesn't work that well.
I wanted to maintain a fork on my own, but most of the extension is written in some weird non-POSIX shell, which I'm not that familiar with at all.

I chose to write the “backend” in Rust, mainly to discover, and because I felt like it was more suited for this than Haskell.

## Features

TODO

## Dependencies

Compile-time:
- A Rust installation with `cargo`, to build the daemon

Runtime:
- coqidetop (should come with a Coq installation by default -- only tested for 0.13.2)
- [socat](https://linux.die.net/man/1/socat)

> :warning: Will not work for Coq versions 0.14.+, as the XML protocol has changed.
> This is something that should be handled dynamically through a call to the `Version` call at initialisation time.

## Installation

The recommended way to install this plugin is using [plug.kak](https://github.com/andreyorst/plug.kak), though other means are also available.

### With plug.kak

Put this in your `kakrc` file.

```kak
plug "mesabloo/coqide.kak" do %{
  cargo build --release --locked
  cargo install --force --path . --root ~/.local
} config %{
  # configure this plugin here
}
```

After that, run `:plug-install` from within Kakoune and everything should be correctly installed.
Note that if `~/.local/bin` is not in your PATH, you will need to edit the option `coqide_tools_folder`, by putting this in the `config` block above:

```kak
set-option global coqide_tools_folder "~/.local/bin"
```

### Using autoload

Clone this repository to your `autoload` directory.
Note that you will need to manually build the daemon using the two above commands.
Further configuration is done within your `kakrc` file.

### Manual installation

Clone this repository somewhere and `source` the files `rc/coqide.kak` and `rc/syntax.kak` (better syntax highlighting) in your `kakrc`.
The rest of the procedure is already described above.

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
- `coqide-hints` asks the daemon for hints for the current proof.
  These may not necessarily be meaningful or useful at all, but this command is provided just in case.
- `coqide-goto-tip` moves the cursor to the tip.
- `coqide-enable-gutter-symbols` enables the display of little symbols in the gutter to be more visual about errors/axioms.
- `coqide-disable-gutter-symbols` disables the above-mentioned display of symbols in the gutter.
- `coqide-interrupt` allows interrupting the processing of the current (and next) Coq statement(s).

Additional functionality:
- This plugin will also automatically backtrack to the cursor when an insertion is detected before the end of the processed range.

## Documentation

This plugin comes with several default options, but some of them can be altered:

- `coqide_tools_folder` is the folder containing all the required tools written in Rust for this plugin to work.
  This may be left blank, which means that the executables must be in your PATH.

  The indicated folder must contain the executables:
  - `coqide-daemon`: the daemon to serve as a bridge between Kakoune and CoqIDE
  - `coq-parser`: the small tool used to get statement boundaries in the rest of the buffer.

  The default value for this option is the empty string `""`.
  A sane value could be `~/.local/bin`, as found in my example configuration.
- `coqide_gutter_admitted_symbol` is the symbol displayed in the gutter next to any range containing an axiom.
  This defaults to `?` but is quite ugly, so I recommend changing it.
- `coqide_gutter_error_symbol` is the symbol output in the gutter next to an error range.
  This defaults to `!`, but as for the precedent symbol, I recommend changing it.
- **Colors:**
  - Ranges:
    - `coqide_processed_face` is the `face` used to highlight Coq code which has already been processed by the daemon.
      This can be customized as wanted using `set-face`, but comes with the default value `default,green`.
    - `coqide_error_face` is the `face` used to highlight errors in the Coq buffer.
      It defaults to `default,red` and can be customized with `set-face`.
    - `coqide_to_be_processed_face` is the `face` used to color code which is yet to be processed by the daemon.
      Defaults to `default,magenta` to be as close as possible to default CoqIDE colors.
    - `coqide_admitted_face` is the `face` used to highlight parts of the code which contain admitted proofs, as in CoqIDE.
      This defaults to `default,yellow` so as to be visual and mimic CoqIDE.
    - `coqide_error` is the `face` used to color the error messages in the result buffer.
      Defaults to `red+b` for consistency with `coqide_error_face`.
    - `coqide_warning` is the `face` used to color the warning messages in the result buffer.
      Defaults to `yellow+b`.
  - Code coloring:
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
  - Gutter symbols:
    - `coqide_gutter_admitted_face` is the `face` used to colorize the symbol put in the gutter when an axiom is added.
      Defaults to `yellow` to be consistent with the default color for the admitted range.
    - `coqide_gutter_error_face` is the `face` used to add some colors to the error symbols which is put in the gutter.
      Defaults to `red` to be consistent with the default color for the error range.

## Things left to do and known bugs

The codebase is at some locations pretty ugly (e.g. when decoding XML nodes to Rust values).
However, most of it should be at last a little bit documented.

Here are some erroneous or incomplete features:
- Modifying the buffer in normal mode (e.g. by pressing `d`) before the tip does not correctly work
  as Kakoune modifies ranges before the backend as any chance to remove those.
  This also may happen sometimes in insert mode.

  A workaround for now is to backtrack by hand until before your cursor.
- Create a `coqide-version` which returns the version of Coq and the XML protocol.
- Kakoune highlighters do not seem to play well with Unicode characters in source code.
- The goal buffer sometimes displays rules with invalid UTF8 characters.
- Multiline hypotheses in a goal break highlighting completely.
- When lines are appended to the result buffer, colors get lost.
- `coqide-makefile` to generate a `CoqMakefile` just as CoqIDE (using `coq_makefile`).
- The whole codebase (mainly the Rust code) lacks documentation.
  This is crucial.
- `coqide-interrupt` cannot be followed by any backtracking operation, else the whole state gets inconsistent.
- :warning: Bugs are yet to be found! If you find any, please report them.

If you feel like it, feel free to improve this plugin by forking this repository and submitting your patches through pull requests.
Just try not to implementi too many features in the same pull request (two is acceptable, if small).

## Personal configuration

As I intend to use this plugin, here is how I configured it.
It spawns two new kakoune clients containing the result and goal buffers and deletes them when the master buffer gets discarded.

```kak
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
  map global coq t ": coqide-goto-tip<ret>" \
    -docstring "go to the document tip"
  map global coq h ": coqide-hints<ret>" \
    -docstring "ask for some hints"
  map global coq q ": coqide-query<ret>" \
    -docstring "send a query to coqtop"
  map global coq l ": coqide-dump-log<ret>" \
    -docstring "dump logs"
  

  # Create two additional clients to show goals and results
  hook global BufCreate (goal|result)-(.*) %{
    evaluate-commands %sh{
      client_name="${kak_hook_param_capture_1}-${kak_hook_param_capture_2}"

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
      
      echo "new '$switch_to_buffer'"
      echo "hook -once global BufClose '$kak_hook_param_capture_0' %{
        evaluate-commands -client '$client_name' 'quit!'
      }"
    }
  }


  hook global WinCreate .* %{
    hook window WinSetOption filetype=coq %{
      require-module coqide

      # User mode to interact with CoqIDE only in Coq files
      map buffer user c ": enter-user-mode coq<ret>" \
        -docstring "enter the Coq user mode"

      # Enable symbols in the gutter for errors/axioms
      coqide-enable-gutter-symbols
      # -> To disable: coqide-disable-gutter symbols
      set-option global coqide_gutter_error_symbol "█"
      set-option global coqide_gutter_admitted_symbol "░"     # I quite like these ones
      set-face global coqide_gutter_error_face red+b
      set-face global coqide_gutter_admitted_face yellow+br

      # Better looking face
      set-face global coqide_processed_face default,black
      set-face global coqide_error_face default,default,bright-red+c
      set-face global coqide_admitted_face default,default,yellow+cd
      set-face global coqide_to_be_processed_face default,default,black+u

      # Commands to execute when the buffer is closed.
      # These are declared here as a string in order to have the value of `%opt{coqide_pid}`
      # (which is an internal option)
      set-option buffer coqide_close_panels "
        evaluate-commands -client goal-%opt{coqide_pid} 'quit!'
        evaluate-commands -client result-%opt{coqide_pid} 'quit!'
        remove-hooks coqide-%opt{coqide_pid}
      "

      # Remove the side panels when closing the buffer
      hook global -group "coqide-%opt{coqide_pid}" BufClose "%val{buffile}" %{ try %opt{coqide_close_panels} }
      hook global -group "coqide-%opt{coqide_pid}" ClientClose .* %{ try %opt{coqide_close_panels} }
    }
  }
}
```
