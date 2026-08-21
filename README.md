# clipi

A small clipboard history manager. It lives in the tray, watches what you copy, and lets you pick an older clip with a hotkey.

Works on Windows and macOS. Windows builds are on the [Releases](https://github.com/Baconiser/clipi/releases) page.

## Usage

Copy text or images as usual. clipi stores them in the background.

- **Alt+C** (Option+C on macOS) opens the palette. Press it again to hide.
- Type to search. Arrow keys move. **Enter** puts the selected clip back on the clipboard.
- **Delete** removes a clip. **Escape** closes the window.
- The tray icon has Show, Settings, and Quit.

The palette remembers its position if you drag it.

## Settings

Open Settings from the tray menu.

| Setting | Default | Range |
| --- | --- | --- |
| Max history entries | 500 | 10 to 10,000 |
| Max database size | 80 MB | 8 to 512 MB |

You can also point the SQLite database at a different file. Existing clips stay in the old file.

Identical copies are stored once. Oldest clips are dropped when the entry or size limit is hit.

## Data

Config and database live here:

- Windows: `%APPDATA%\clipi\`
- macOS: `~/Library/Application Support/clipi/`

That folder contains `config.toml` and `clipi.db`.

## Build

Needs a recent [Rust](https://www.rust-lang.org/tools/install) toolchain.

```
cargo build --release
```

The binary is `target/release/clipi` (or `clipi.exe` on Windows).
