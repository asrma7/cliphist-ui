# Cliphist UI

`cliphist-ui` is a native GTK4 clipboard history popup for Wayland. It is a searchable, keyboard-first frontend for an existing [`cliphist`](https://github.com/sentriz/cliphist) clipboard history setup.

The app does not record clipboard history by itself. It reads entries from `cliphist`, decodes selected entries with `cliphist decode`, and copies them back to the Wayland clipboard with `wl-copy`.

## What It Does

- Shows `cliphist list` entries in a compact popup.
- Filters clipboard history with a search field.
- Supports normal/list mode and insert/search mode.
- Copies the selected entry back to the Wayland clipboard.
- Deletes a selected history entry.
- Clears all history after a two-step confirmation.
- Lazily generates image thumbnails off the GTK main thread.
- Caches image thumbnails under `$XDG_CACHE_HOME/cliphist-ui/thumbnails` or `~/.cache/cliphist-ui/thumbnails`.
- Prunes stale thumbnails after successful history reloads.
- Can run once per invocation or stay resident with `--service`.

## Wayland Requirements

`cliphist-ui` is Wayland-focused. It creates a Wayland layer-shell overlay surface rather than a normal desktop window.

Runtime requirements:

- `cliphist`
- `wl-copy` from `wl-clipboard`
- GTK4 runtime libraries
- GTK4 layer-shell runtime libraries
- A Wayland compositor with layer-shell support

Build requirements:

- Rust 2021 toolchain
- GTK4 development files
- `gtk4-layer-shell-0` development files discoverable by `pkg-config`

## Install

Install the latest GitHub Release binary:

```sh
curl -fsSL https://raw.githubusercontent.com/asrma7/cliphist-ui/main/install.sh | sh
```

This installs `cliphist-ui` to:

```text
~/.local/bin/cliphist-ui
```

Install a specific release:

```sh
curl -fsSL https://raw.githubusercontent.com/asrma7/cliphist-ui/main/install.sh | sh -s -- --version v0.1.0
```

Install under another prefix:

```sh
curl -fsSL https://raw.githubusercontent.com/asrma7/cliphist-ui/main/install.sh | sh -s -- --prefix /opt/cliphist-ui
```

Install under `/usr/local`:

```sh
curl -fsSL https://raw.githubusercontent.com/asrma7/cliphist-ui/main/install.sh | sudo sh -s -- --system
```

The release binaries still require the runtime dependencies listed above. The installer checks for common missing dependencies and prints distro-specific hints.

## Uninstall

Remove a user install:

```sh
curl -fsSL https://raw.githubusercontent.com/asrma7/cliphist-ui/main/install.sh | sh -s -- --uninstall
```

Remove a system install:

```sh
curl -fsSL https://raw.githubusercontent.com/asrma7/cliphist-ui/main/install.sh | sudo sh -s -- --system --uninstall
```

## Build

```sh
cargo build --release
```

The release binary is created at:

```sh
target/release/cliphist-ui
```

## Run

Open the popup:

```sh
target/release/cliphist-ui
```

Start a resident service:

```sh
target/release/cliphist-ui --service
```

In service mode, the primary GTK application stays alive without presenting the popup. Later `cliphist-ui` invocations are routed to the running primary instance and toggle the existing popup. Closing, quitting, or copying hides the popup instead of destroying the process.

Reload the resident service config and CSS without killing the app:

```sh
pkill -SIGUSR1 cliphist-ui
```

This re-reads `config.json5`, `style.css`, and any CSS files imported by `style.css` in the running `--service` process.

## Keyboard

The app starts in normal/list mode unless `behavior.start_in_insert` is enabled.

Insert/search mode:

- Type to filter clipboard entries.
- `Esc` leaves search focus and returns to normal/list mode without clearing the query.

Normal/list mode:

- `j` or Down: move selection down.
- `k` or Up: move selection up.
- `g`: select first visible entry.
- `G`: select last visible entry.
- `/`: focus search.
- `?`: toggle expanded keybind help.
- `Enter` or `y`: copy selected entry.
- `d`: delete selected entry.
- `D`: ask to clear history; press `D` again to confirm.
- `r` or `Ctrl-r`: reload history.
- `q` or `Esc`: close the popup.

## Configuration

The app works without a config file. Optional structured config is loaded from:

```text
$XDG_CONFIG_HOME/cliphist-ui/config.json5
```

or:

```text
~/.config/cliphist-ui/config.json5
```

Missing sections and fields use defaults. If one section is invalid, only that section falls back to defaults. If the JSON5 file itself is invalid, the whole config falls back to defaults.

This is a JSON5 config, so comments and trailing commas are allowed.

```json5
{
  window: {
    width: 760,
    height: 620,
    position: "center",
    offset_x: 0,
    offset_y: 0,
  },

  search: {
    placeholder: "Search clipboard...",
  },

  list: {
    max_text_chars: 180,
  },

  image: {
    width: 260,
    height: 140,
    show_details: true,
    preserve_aspect_ratio: true,
    rounded_corners: true,
    concurrent_jobs: 3,
  },

  behavior: {
    close_on_copy: true,
    reload_on_open: true,
    start_in_insert: false,
    show_keybinds: true,
    show_vim_mode: true,
    click_to_copy: "single",
  },
}
```

### `window`

Controls popup size and layer-shell placement.

- `width`: popup width in pixels. Default: `760`.
- `height`: popup height in pixels. Default: `620`.
- `position`: layer-shell anchor position. Default: `"center"`.
- `offset_x`: horizontal margin in pixels when `position` is `"offset"`. Default: `0`.
- `offset_y`: vertical margin in pixels when `position` is `"offset"`. Default: `0`.

Supported `position` values:

- `"center"`
- `"offset"`
- `"top"`
- `"top-left"`
- `"top-right"`
- `"bottom"`
- `"bottom-left"`
- `"bottom-right"`

All named positions except `"offset"` ignore `offset_x` and `offset_y` and use fixed layer-shell anchors. Use `"offset"` to place the popup from the top-left corner with `offset_x` and `offset_y` as non-negative margins.

### `search`

Controls search field content.

- `placeholder`: placeholder text shown in the search entry. Default: `"Search clipboard..."`.

### `list`

Controls clipboard entry parsing/display behavior.

- `max_text_chars`: maximum normalized preview length for text rows. Default: `180`.

Long text entries are still copied in full; this only affects the visible row preview.

### `image`

Controls thumbnail generation and image row behavior.

- `width`: thumbnail target width in pixels. Default: `260`.
- `height`: thumbnail target height in pixels. Default: `140`.
- `show_details`: show the image detail label, such as `Image 542x422`, above thumbnails. Default: `true`.
- `preserve_aspect_ratio`: preserve image aspect ratio when generating thumbnails. Default: `true`.
- `rounded_corners`: write transparent rounded corners into generated thumbnail PNGs. Default: `true`.
- `concurrent_jobs`: maximum thumbnail worker count, clamped internally. Default: `3`.

Thumbnails are generated lazily for visible/nearby image entries and cached on disk.

### `behavior`

Controls popup and keyboard behavior.

- `close_on_copy`: close or hide the popup after a successful copy. Default: `true`.
- `reload_on_open`: reload `cliphist list` when the popup opens. Default: `true`.
- `start_in_insert`: start with search focused. Default: `false`.
- `show_keybinds`: show footer keybind hints. Default: `true`.
- `show_vim_mode`: show the compact normal/insert mode indicator. Default: `true`.
- `click_to_copy`: copy a row on `"single"` or `"double"` click. Default: `"single"`.

## Styling

Visual styling is handled through GTK CSS, not `config.json5`.

The app always installs built-in CSS defaults first. Optional overrides are then loaded from:

```text
$XDG_CONFIG_HOME/cliphist-ui/style.css
```

or:

```text
~/.config/cliphist-ui/style.css
```

This makes generated themes straightforward: tools can write `style.css` directly with normal GTK CSS selectors.

Example:

```css
window.cliphist-ui-window {
  color: #dee4e0;
  font-family: Inter, Cantarell, sans-serif;
  font-size: 14px;
}

.cliphist-root {
  background: #0f1512;
  border: 2px solid #27302b;
  border-radius: 10px;
  padding: 10px;
}

.cliphist-search {
  background: #141c18;
  color: #dee4e0;
  border-radius: 10px;
}

.cliphist-list row {
  border-radius: 10px;
  margin-bottom: 6px;
  padding: 8px 10px;
}

.cliphist-list row:selected {
  background: #a9cbe2;
  color: #0e3446;
}

.cliphist-mode-normal {
  background: #8f9a95;
}

.cliphist-mode-insert {
  background: #a9cbe2;
}

.cliphist-keyhint-key {
  background: #141c18;
  border: 1px solid #27302b;
  border-radius: 10px;
}

.cliphist-status,
.cliphist-muted,
.cliphist-keyhint-action {
  color: #8f9a95;
}

.cliphist-thumb-placeholder {
  background: #18201c;
  border-radius: 8px;
}
```

Stable selectors intended for user or generated themes:

- `window.cliphist-ui-window`
- `.cliphist-root`
- `.cliphist-header`
- `.cliphist-search`
- `.cliphist-list`
- `.cliphist-list row`
- `.cliphist-list row:selected`
- `.cliphist-kind`
- `.cliphist-preview`
- `.cliphist-muted`
- `.cliphist-footer`
- `.cliphist-mode`
- `.cliphist-mode-normal`
- `.cliphist-mode-insert`
- `.cliphist-hints`
- `.cliphist-keyhint`
- `.cliphist-keyhint-key`
- `.cliphist-keyhint-action`
- `.cliphist-status`
- `.cliphist-thumb-placeholder`

## Clipboard Backend Boundary

All clipboard history operations are delegated to command-line tools:

- `cliphist list`
- `cliphist decode`
- `cliphist delete`
- `cliphist wipe`
- `wl-copy`

`cliphist-ui` does not run clipboard watchers, store history itself, sync history, or manage background clipboard capture.
