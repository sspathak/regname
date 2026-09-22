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

- `-c`, `--confirm`, `-i`, `--interactive`: Prompt for confirmation with a pop-up modal before executing rename (`"Are you sure you want to execute this rename?"`).
- `-p`, `--prevent-delete`, `-n`, `--no-overwrite`: Prevent rename if the before and after file count decreases (protects against overwriting or deleting files due to colliding regex patterns).
- `-s`, `--safe`: Safe mode: enables both `--prevent-delete` and `--confirm`.
- `-h`, `--help`: Show help.

### Controls

- `TAB`: Select either the "Match" input field or "Rename" input field
- `CTRL+C`: Cancel and exit
- `ENTER`: Rename all matching files (or open confirmation pop-up if `-c`/`-s` is set)
- In Confirmation pop-up:
  - `y` or `Y`: Confirm and execute rename
  - `n`, `N`, or `ESC`: Dismiss dialog and return to editing (does not exit tool)
  - `TAB`, `LEFT`, `RIGHT`: Toggle between Yes / No buttons
  - `ENTER`: Choose selected button
- In Blocked pop-up (when file count decreases with safe mode):
  - `ENTER`, `ESC`, `SPACE`: Dismiss error pop-up and return to editing

## :page_facing_up: License

This project is released under the terms of the [MIT License](./LICENSE.txt).
