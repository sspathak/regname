# regname

Mass renamer TUI written in Rust.

[![asciicast](https://asciinema.org/a/711473.svg)](https://asciinema.org/a/711473)

## :package: Installation

```bash
cargo install --locked --git https://github.com/linkdd/regname
```

## :memo: Usage

```bash
regname [OPTIONS] [path/to/a/folder]
```

### Options

Safe mode is **enabled by default** (prompts for confirmation and blocks file overwrites/collisions).

- `-y`, `--no-confirm`, `--yes`: Skip the confirmation pop-up dialog.
- `-f`, `--force`, `--overwrite`: Allow renaming even if files would be overwritten or deleted.
- `--unsafe`: Bypass all safety checks (disables both confirmation and overwrite guard).
- `-c`, `--confirm`, `-i`, `--interactive`: Require confirmation pop-up (enabled by default).
- `-p`, `--prevent-delete`, `-n`, `--no-overwrite`, `-s`, `--safe`: Block overwriting/deletion (enabled by default).
- `-h`, `--help`: Show help.

### Controls

- `TAB`: Select either the "Match" input field or "Rename" input field (defaults: `(.*)` and `${1}`)
- `CTRL+R`: Open command history modal to pick from previously executed regex pairs
- `CTRL+C` or `ESC`: Exit regname
- `ENTER`: Open confirmation pop-up (or execute immediately if `--no-confirm` or `--unsafe` is used)
- In History pop-up (`CTRL+R`):
  - `UP` / `DOWN`: Browse through previous commands
  - `CTRL+R`: Cycle to next command
  - `ENTER`: Load selected match and rename patterns into input fields
  - `ESC`: Dismiss history picker
- In Confirmation pop-up:
  - `y` or `Y`: Confirm and execute rename
  - `n`, `N`, or `ESC`: Dismiss dialog and return to editing (does not exit tool)
  - `TAB`, `LEFT`, `RIGHT`: Toggle between Yes / No buttons
  - `ENTER`: Choose selected button
- In Blocked pop-up (when file collisions/count decrease are detected):
  - `ENTER`, `ESC`, `SPACE`: Dismiss error pop-up and return to editing

## :page_facing_up: License

This project is released under the terms of the [MIT License](./LICENSE.txt).
