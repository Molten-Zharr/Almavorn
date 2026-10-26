<p align="center">
  <img src="docs/images/almavorn-logo.png" width="192" height="192" alt="Almavorn logo">
</p>

# Almavorn

[English](README.md) | [Русский](README.ru.md)

### INSTALL: [curl](#curl) - [AppImage](#appimage) - [Flatpak](#flatpak) - [Installation guide](#download-and-install)

<p align="center">
A local music player for Linux, inspired by AIMP and Strawberry.
</p>

**Almavorn** is built for comfort and control:

- Use your keyboard, your mouse, or both at once!
- An equalizer, playback settings, and background animations!
- Tooltips, action descriptions, and customizable shortcut highlighting!
- Remap keys, move panels, choose your own colors, and edit track titles and metadata.
  **It's all built in!**
- Two modes: **Order** protects playlists from accidental edits.
  Switch to **Chaos** to add tracks straight to playlists and folders!

Suggestions and feedback are welcome. We're only getting started, and we hope you'll enjoy the player!

![Almavorn in English: a protected playlist in Order mode](docs/images/almavorn-order-en.png)

## Order, Chaos, and the sorting desk: what's the difference?

- **Order: "my collection, kept intact".** Playlists are protected from accidental changes. Choose "Edit" to make changes, then "Done" to lock the playlist again. Sorting the view preserves the saved track order.
- **The sorting desk is a key part of Order.** It's a shared playlist available in both modes: add files, correct titles, tags, and artwork, then copy tracks into the playlists you want!
- **Chaos: free-form playlists.** Rearrange tracks and build new playlists from existing ones. History keeps the last 100 actions until you close the player, so you can undo and redo them.

The tag editor writes changes to the audio file itself. Those changes affect that file in every playlist.

![Almavorn in English: a free mix in Chaos mode with sparks in the background](docs/images/almavorn-chaos-en.png)

## What else?

- Find your music with search, filters, and sorting.
- Build playlists yourself or from folders, with or without subfolders.
- Drag playlists into order and combine them into new mixes!
- Use "Add new files" to find new tracks in linked folders.
- Listening statistics for every track! Double-click an entry to find that song in its playlist.
- Tune your sound with an equalizer and ten presets to choose from.
- Play tracks in reverse, adjust speed, tempo, pitch, and bass, and save different settings for every track!
  (now you can turn your track into that version from a hardstyle mix)
- Control playback with media keys while the window is minimized. Set global shortcuts in Settings.
- Choose themes, waveform styles, background animations, and settings profiles with your own shortcuts.
- Arrange blocks your way: drag their titles, resize them, and hide what you don't need!
- Read lyrics in the player and save them for offline listening.
- Edit titles, tags, and artwork for several tracks at once.
- Choose a cover thumbnail size and open the original with one click.
- Drag files straight from your file manager into the GUITUI window.
- Find and download music by name or link through "Downloads".
- Connect music services and listen online in the same window!
- Open keyboard help while your music keeps playing.
- Type with the on-screen keyboard and control the player with your mouse!
  (you can lie back with your head on a pillow and control everything with one hand)
- Mark just the tracks you want in a playlist and listen only to those!

## Themes, appearance, and usability

"Layout" mode lets you rearrange the interface. For example, you can put the player at the bottom, playlists on the right, and give most of the window to the track list.

Click "Layout" or press Ctrl+B. Controls will appear on the panel headers:

- "[↕]" lets you drag a block. You can also drag its title.
- "[-]" collapses a block; "[+]" expands it again.
- "[x]" hides a block.

Where you drop a block determines what happens: drop it at the edge of another panel to place it beside that panel, or in the center to swap them. Drag the shared border between panels to resize them.

"Visibility…" opens the list of blocks and buttons. You can bring back a hidden panel or hide individual buttons you don't need. "Restore default layout" is there too.

You can also use the keyboard: select a block with Tab, then use Ctrl+Alt+arrow keys to swap it with a neighboring block, or Ctrl+Shift+arrow keys to resize it. Ctrl+Alt+Space collapses or expands the block; Ctrl+Alt+Delete hides it.

Your layout is saved automatically. Click "Done" or press Ctrl+B to finish. During a drag, Esc cancels that move; otherwise, it exits layout mode.

## Download and install

The packaged release is **0.1.4**, for 64-bit Linux on Intel/AMD.

*(Windows support is in the works, hehe)*

### GUITUI version

**GUITUI** runs in its own window with mouse and keyboard controls:

[AppImage](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.AppImage) - [Flatpak](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.flatpak) - [tar.gz](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.tar.gz) - especially handy on KDE/GNOME/COSMIC

---OR---

<a id="curl"></a>

```sh
curl -fsSL https://raw.githubusercontent.com/Molten-Zharr/Almavorn/main/install.sh | bash -s -- --variant guitui
```

### TUI version

**TUI** runs right inside your favorite terminal:

[AppImage](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.AppImage) - [Flatpak](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.flatpak) - [tar.gz](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.tar.gz)

---OR---

```sh
curl -fsSL https://raw.githubusercontent.com/Molten-Zharr/Almavorn/main/install.sh | bash -s -- --variant tui
```

---

*- Psst, which one should I pick?*

*- Here's the difference:*

**curl** downloads and runs the installer from our repository for your chosen interface!
Paste the command straight into your terminal.

<a id="appimage"></a>

**AppImage** is a single file you can run without installing. [GUITUI](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.AppImage) · [TUI](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.AppImage).

<a id="flatpak"></a>

**Flatpak** installs the app with its required components. [GUITUI](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.flatpak) · [TUI](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.flatpak).

**tar.gz** is an archive with an installer and an application menu shortcut.

**How to update:** If you installed the player with the command above, run it again; otherwise, download the same type of file you chose originally.

## First launch

F2 opens Settings; F1 opens the full help guide. Within Settings, F1 explains the selected option.

### Create a playlist from a folder

1. At the top, open "Add to desk" and choose "Folders to playlists".
2. Find your music folder. Double-click or press Enter to open a folder; "Parent" takes you up one level. The current path appears at the top of the window, for example "/home/anna/Music".
3. Check the box next to a folder, or select it and press Insert. You can mark several folders at once: each becomes its own playlist!
4. Enable "Subfolders as groups" to include music from nested folders. Without it, only files directly inside the marked folders are added.
5. Click "Create playlists" or press Ctrl+Enter. The playlists will appear in the list, named after the folders you selected.
6. Open a playlist and double-click a track, or select it and press Enter. Space pauses and resumes playback.

### Build a playlist yourself

1. Open "Playlist" and choose "New playlist", or press C. Enter a name and press Enter to create an empty playlist.
2. In Order mode, select it and click "Edit".
3. Open "Add to desk", choose "Add files", mark the files you want, and click "Add to desk" at the bottom. For a whole folder, choose "Add folder", open that folder, then click "Current folder · Add to desk".
4. On the sorting desk, mark tracks using their checkboxes or Insert. Press X, choose your playlist, and confirm with Enter. The tracks will be copied into the playlist; the original files stay where they are.
5. Return to the playlist and click "Done" to protect it from accidental changes again.

The sorting desk helps you organize music before adding it to permanent playlists. In Chaos, you can add files straight to the open playlist: uncheck "Add to sorting desk" in the add menu.

### Other useful controls

- F4 opens the equalizer; F5 opens speed, tempo, and pitch settings.
- F7 opens statistics, F8 the tag editor, and F9 song lyrics.
- / searches playlists; Ctrl+F filters the current playlist.
- Ctrl+B enables layout editing. Drag a block by its title, drag a border to resize it, then click "Done".
- Insert marks tracks, Ctrl+Enter plays only the marked tracks, and Ctrl+U returns to the whole playlist.
- Ctrl+Z undoes an action; Ctrl+Y redoes it, in Chaos and on the sorting desk.
- Tab switches panels; right-click opens the action menu. Esc closes the open window; in the file picker, it clears the filter first.

These are the default keys. Set your own shortcuts under F2, "Shortcuts".

[All releases](https://github.com/Molten-Zharr/Almavorn/releases) · [Keyboard shortcuts](docs/GLOBAL_SHORTCUTS.md) · [MIT license](LICENSE)
