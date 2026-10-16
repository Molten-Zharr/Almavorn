# Almavorn

[English](README.md) | [Русский](README.ru.md)

Stack: Rust, Ratatui, Tokio, Rodio, SQLite, Crossterm, egui, eframe, egui_ratatui.

Linux releases include both graphical and terminal executables. Download the
archive and SHA256SUMS from [GitHub Releases](https://github.com/Molten-Zharr/Almavorn/releases).
See [installation, system requirements, and release builds](docs/RELEASE.md).
The project is [MIT licensed](LICENSE); dependencies retain their own licenses.

Build and run:

```sh
cargo run --release
cargo run --release --no-default-features --features tui --bin almavorn-tui
```

SoundTouch is built from its bundled source for tempo and pitch processing.
Building requires a C++ compiler and libclang with its standard headers. On
Debian/Ubuntu, install `build-essential libclang-dev` (and `libasound2-dev`
for Rodio). The bundled SoundTouch library uses LGPL-2.1; its source and
license are included in the `soundtouch-ffi` crate. No separately installed
SoundTouch library is needed to run the player.

SQLite is compiled and embedded in the executables through rusqlite. No
DuckDB runtime library is required. The active database is `almavorn.sqlite`.
On first launch, an existing `almavorn.duckdb` is imported using an installed
DuckDB CLI; otherwise `almavorn.sqlite3` is imported if present. Both original
files are preserved. If DuckDB import fails, the application stops without
falling back to an older library. DuckDB CLI is only needed for this one-time
import. The DuckDB application is preserved in the `archive/duckdb` branch.

Both interfaces share application state and Ratatui rendering:

- `src/app.rs`: shared state, initialization, and the update loop.
- `src/app/context.rs`: library (`app.library`), playback (`app.playback`), and
  interface (`app.view`) state. Read the library through `app.playlists()`;
  library reloads control row-cache invalidation.
- `src/app/background.rs`: background operations and a latest-request queue
  for folder reads and audio preparation that discards obsolete results.
- `src/app/`: actions and input, library and undo history, background database
  operations and import,
  playback, dialogs, and command-line options.
- `src/ui.rs`: main-screen layout.
- `src/ui/`: library and player panels, themes, shared widgets, and dialogs.
- `src/ui/buttons.rs`: shared measurement, shortcuts, and hit registration,
  with separate visual variants for dialog and command buttons.
- `src/store.rs`: SQLite storage with WAL, serialized background writes, and legacy migration.
- `src/store/queries.rs`: two-query library loading and scoped undo snapshots
  that do not read track metadata.
- `src/errors.rs`: typed errors with English and Russian messages.
- `src/preferences.rs`: profiles, palettes, theme presets, and palette exchange.

Displayed track filtering and sorting are cached until the library, selected
playlist, query, or sort options change. The command bar wraps to fit narrow windows.

## Main screen

The command bar provides Add, Playlist, undo/redo, search, Statistics, Settings, and the
current mode's menu. `Tab` and `Shift+Tab` cycle through the panels and command
bar. On the bar, arrows select an enabled button and `Enter` opens it; `Esc`
returns to the library. The focused button is highlighted, and keyboard menus
open below it. The `+` beside Playlists creates a playlist. The `...` in
panel headings and right click open actions for the selected playlist, track,
or player. `F10` opens the focused panel's menu; use arrows, Enter, and Esc to
navigate. Clicking outside dismisses it. Shortcuts appear in menus and the
status line when hovering over a button.
Menu navigation skips unavailable commands and stops at the first or last
available command. Help scrolls by rendered lines and stops at the last full
page, including after resizing. GUI keycaps are painted separately from the
text raster; TUI shows bracketed key labels.

`Ctrl+B` or Layout enables arranging: move, collapse, and close controls
appear, and borders become draggable. Visibility opens the existing panel
and individual command choices. `Ctrl+B`, Esc, or Done finishes arranging.
Older layouts migrate to three content panels; command blocks become menus
while hidden command preferences are preserved.

**Add → Folders to playlists** (`Ctrl+Shift+F`) opens a folder picker with
checkboxes. Check folders with the mouse, Space or Insert; Enter or double-click
opens a folder. Marks remain when navigating. `Ctrl+A` checks folders in the
current view, and `Ctrl+D` clears all marks. **Create** makes a separate playlist
for each checked folder, using its name; occupied names receive a numeric suffix.
Only files directly in each folder are included by default. **Subfolders as
groups** (`Ctrl+G`) includes their music as groups inside each new playlist.
Creation runs in the background and commits all new playlists together;
Chaos supports undo and redo of the whole addition.

The idle player is compact; playback reveals the waveform. Position and
volume sliders support dragging. Main panels use subdued border colors; border type and corners follow
Themes → Font and geometry. Color highlights selection and active headings. New configurations default
to regular Fira Code; existing font choices remain intact.

The player has repeat and shuffle buttons, also available in its context menu.
Single-click the repeat button to enable track repeat; single-click again to
turn it off. Double-click enables playlist repeat.
`L` cycles repeat off → playlist → track; `H` toggles shuffle. Track repeat
restarts a finished track; Next still skips it. Playlist repeat wraps at the
ends of the queue. Shuffle chooses randomly from every other track in the
playing playlist, even when a different playlist is selected; Previous returns
through playback history. Track repeat takes priority over shuffle. Shuffle
with a single track stops unless track repeat is enabled. Modes and shortcuts
are saved in the active settings profile.

Track checkboxes can also define a temporary repeating set inside a playlist.
Choose **Play checked tracks only** from the tracks context menu, or press
`Ctrl+Enter`. Checkboxes change the active set immediately. Removing the playing
track advances to the next checked track; clearing all stops playback. The set
keeps playing when browsing other playlists, and its checkboxes return when
selecting its playlist again. It repeats even a single track, independently of
the saved repeat setting; shuffle stays within the checked set. **All tracks**
in the player, **Play the whole playlist** in either context menu, or `Ctrl+U`
returns to the full playing playlist without restarting the current track.
Stop, Enter on a track, or double-click also exits this mode. This selection
exists only for the current playback session; it creates no separate playlist.

`F4` or the player's EQ button opens the equalizer; the player menu lists
Equalizer with its assigned shortcut. Ten bands cover 32 Hz to 16 kHz, plus a
preamp for overall gain, all adjustable from −12 to +12 dB. Drag the sliders
or use the wheel in 1 dB steps. Left/Right or Tab selects a slider, Up/Down
changes its gain, Space switches EQ on/off, Enter opens presets, and R resets
gains. The eight presets (Flat, Dance, Rock, Pop, Classical, Bass boost,
Treble boost, Vocal) also have shortcuts 1–8. Choosing a preset preserves
the on/off state. Manual changes display Custom and are saved with the
active profile. Adjustments affect the playing track without restarting it;
small windows use horizontal sliders and keyboard navigation scrolls them.

`F5` or the player's FX button opens Playback tuner; its menu entry shows
its assigned shortcut. Speed, tempo and pitch range from 50% to 200%, with
100% as the original value. Speed changes duration and pitch together;
tempo changes duration while preserving pitch; pitch changes pitch while
preserving duration. The pitch slider also shows the shift in semitones.
Bass ranges from 0% to 100%, corresponding to a 120 Hz low shelf of 0–12 dB,
and works independently of the equalizer's on/off state.

Drag sliders or use the wheel in 1% steps. Up/Down or Tab selects a slider,
Left/Right changes it by 1%, PageUp/PageDown by 10%, and Home restores its
normal value. Space toggles reverse playback and R resets all tuner settings.
Speed, tempo, pitch and bass apply during playback; the timeline remains in
original track time. Reverse starts new tracks from the end, or reverses the
current track from its current position, retaining pause state. Preparing
reverse audio runs in the background and uses an automatically removed
private PCM file with bounded memory; subsequent seeks reuse that cache.
Global tuner settings save in the active profile. Enable **Pin FX to this track**
(P) while a track is playing to save its speed, tempo, pitch, bass and reverse
separately. Further edits and Reset affect only that track. Its FX load whenever
the same file plays, in any playlist or profile, and persist after restarting.
Other tracks use their own pinned FX or the active profile's global settings.
Disable the pin to remove the track's FX and immediately return to global settings.
Narrow windows scroll the sliders when navigating with the keyboard.

The main window background responds to the playing audio's level, frequency
bands and attacks after EQ/FX processing. Choose spectrum, waveform, pulse
rings, particles or Carpet bombing, inspired by the Red Alert 3 menu: a red
city skyline, bomber formations, falling bomb trains, successive blasts and
smoke. Carpet bombing is enabled by default. VIS opens a dropdown with the
current scene marked: spectrum, waveform, rings, particles, bombing, then
No animation as the last option. `F6` cycles through them.
Settings → General selects the scene and brightness (10–70%);
both save in the active profile. Pausing freezes the scene; stopping clears
it. Text, selection and controls remain above the background; settings and
dialogs retain their own backing.

Render animation previews and a bombing frame sequence without a window or
audio device:

```sh
cargo run --release --example visualizer_preview
```

PPM images are written to `target/visualizer-preview/`.

Render the main screen, menus, and layout editor without a window:

```sh
cargo run --release --example interface_preview
```

PPM images are written to `target/interface-preview/`.

## Listening statistics

Click **Statistics** beside Settings on the command bar, open **Listening
statistics** from the current mode's application menu, or press `F7`.
The summary shows total listening time, qualified plays, starts, and unique
tracks, artists and albums. Four tabs list tracks, artists, albums and the last
100 listening sessions; totals include the entire history. Artist and album
counts aggregate track plays. Sort by listening time, play count or name.
Use `←`/`→` or Tab to change tabs, arrows or the wheel to select rows,
`S` to change sorting, `R` to refresh, and Esc or F7 to close.

A track counts as played after **50% of its duration or four minutes of actual
playback**, whichever comes first. Shorter listens still add time. Pauses and
seeking do not add time; EQ/FX playback time is measured from rendered audio.
Each repeat starts a new session, and rewinding cannot count a session twice.
History begins with this version, is shared between modes and profiles, and
persists in SQLite independently of playlist undo. Checkpoints save in the
background every five seconds (every second with the dialog open), on track
changes and when closing the application.

Render the dialog without a window or audio device with
`cargo run --release --example statistics_preview`.
PPM images are written to `target/statistics-preview/`.

## Settings

Open Settings from the application menu. Sections appear on the left,
compact parameter rows on the right, and the selected description,
keyboard hints, and autosave state at the bottom. Volume uses a single-line
slider. Values share a column at the right edge; narrow windows put them below
their labels. Clicking a value or pressing Enter changes it or opens its editor;
arrows adjust it. Sections cover general settings, profiles, themes, and shortcuts.
Themes contain three tabs: Presets, Palettes, and Font and geometry.
Open a full parameter description
with `F1`, the info button, or a double click on its row.

- `Tab` moves focus between sections, theme tabs, and parameters; `Shift+Tab`
  moves backwards. `↑`/`↓` select a row,
  `←`/`→` change a value, `Enter` runs an action, and `Esc` goes back.
- Use the mouse to select pages and rows, change values with buttons,
  scroll lists, and enter text through the on-screen keyboard.
  Sections, tabs, parameter rows, buttons, and breadcrumbs highlight when hovered.
  Moving the pointer over a section or tab makes it current. Hovering over
  parameters only highlights them; the selected parameter stays fixed while
  moving to the Edit or Info buttons. Click a parameter or use the keyboard
  or wheel to change the selection.
  Left-click adjustable values to increase or advance; right-click to decrease
  or go backwards. The wheel moves one item per event in the panel under the pointer.
  Click or drag the volume scale; its percentage appears underneath.
  Click breadcrumbs to return to the application, general settings, profiles,
  or the Themes section.
- In catalogs, `Ins` creates a copy, `F3` renames, and `Del` requests deletion.
  On the palettes page, `Ctrl+I` imports and `Ctrl+E` exports JSON.

Changes save automatically to the active profile. Each profile stores the
language, volume, layout, key bindings, and appearance. Switching profiles
saves the previous preferences and preserves the music library. Palette and
preset catalogs are shared: editing a palette updates every theme and profile
that uses it.

**Molten-Zharr** is the first theme and palette, containing all 16 color roles
from the organization's palette. Its source is bundled in
[`assets/brand/molten-zharr.json`](assets/brand/molten-zharr.json), with role
descriptions in [`assets/brand/PALETTE.md`](assets/brand/PALETTE.md).
Classic Amber, Classic Violet, and Light are also available. Order and Chaos
can use different palettes. Presets capture the palette, font, size, borders,
and corners; create, update, rename, and delete them from the themes page.
Choosing a preset applies and saves it immediately, without a separate Apply
button. The displayed preset follows the current mode and profile, including
after a restart. Modified appearance that does not match a saved preset is
shown as Custom.
Existing configuration colors migrate into Legacy palettes.

Choose Fira Code Bold, Fira Code, or DejaVu Sans Mono at 10–32 pixels, with
no borders, single, double, or thick borders. Rounded corners apply to single
borders. Font and size changes take effect immediately in GUITUI; the terminal
controls them in TUI. Bundled font licenses are in `assets/fonts/`.

Copy, rename, edit colors, and delete palettes. Click a color sample or HEX value
to open the color wheel with hue, saturation, brightness, and RGB sliders.
HEX input stays synchronized. The original sample resets the draft; Apply saves
the color, and Cancel discards it. Use Tab to choose a control, arrows for a
single step, and Shift+arrows for ten steps. Scrolling over a slider adjusts it.
Import accepts Almavorn
JSON (`almavorn.palette`, version 1) and Molten-Zharr palette JSON (version 2),
adding an independent copy. Import, export, and deletion appear under the
Danger zone divider. In both GUI and TUI, import opens a built-in JSON file
picker; export lets you choose a folder and enter just a file name (`.json`
is added automatically). Click to select, Enter or double-click to open a folder
or import a file, Backspace to go up, and Ctrl+Enter to confirm the selection.
Home and Documents buttons provide quick navigation. The last chosen folder is
remembered for the session; Ctrl+F in the export name dialog selects another
folder. Export creates a new file and preserves existing files.
Removing a palette redirects its references to a
remaining palette. Each catalog retains at least one entry.

Render every settings page without a window or audio device:

```sh
cargo run --release --example settings_preview
```

PPM images are written to `target/settings-preview/`.

Regression tests cover playlist editing, undo/redo, background import, profiles
and palettes, settings migration, input, rendering, row-cache invalidation,
shared-track protection, isolated undo snapshots, and stale background results.
They do not require
an audio output device:

```sh
cargo test --release --all-features
cargo test --release --no-default-features --features tui
```
