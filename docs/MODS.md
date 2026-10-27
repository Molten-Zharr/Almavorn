# Almavorn mods

Open "Mods" in the application menu to install, create, edit, enable, disable or remove a mod.

## First mod

Choose "Create" for a working example, edit its code, then save with Ctrl+S. Open a mod to use its custom buttons and scroll its output. "Archive" exports a shareable ZIP without personal settings or state.

## Mod files

A package contains manifest.json and main.rhai at its root. Installing the same ID updates the mod while keeping its settings and state. The installation dialog shows the requested permissions. Removal uses a confirmation dialog.

```json
{
  "api": 1,
  "id": "my-player-tools",
  "name": "Player tools",
  "version": "1.0.0",
  "description": "Show the current song or play the next one.",
  "author": "Your name",
  "permissions": ["playback"],
  "hosts": [],
  "actions": [
    {"id": "show", "name": "Current song"},
    {"id": "next", "name": "Next song"}
  ],
  "events": ["track_changed"],
  "settings": {"prefix": "Now playing"}
}
```

Write the handler in [Rhai](https://rhai.rs/book/):

```rust
fn handle(event, context, settings, state) {
    if event == "action:next" {
        return #{commands: [#{action: "next"}]};
    }
    let song = context.playing;
    let caption = if song == () { "Nothing is playing" }
        else { settings.prefix + ": " + song.artist + " — " + song.title };
    #{message: caption, panel: caption}
}
```

## Events and results

The event is "action:ID" for a button, or one of the subscribed "startup", "track_changed" and "tick" events. Tick runs about once per second; pending duplicate events are combined. Context contains playing, selected, position_ms, paused, volume and mode. Songs contain id, title, artist, album and duration_ms; absent songs are (). Settings and state persist between runs.

Return an object with optional message, panel, commands and state fields. Message shows a notification; panel replaces the mod's output; state replaces its stored state.

## Permissions

- playback: play_pause, stop, next, previous, volume and seek commands.
- library_read: context.playlists with playlist IDs, names, track_count and tracks containing entry_id. Up to 128 playlists, 200 entries per playlist and 2000 entries overall are included. File paths and tags are excluded.
- appearance: palette commands; context.palettes lists available IDs and names.
- network: http_json("https://example.org/data.json"), restricted to exact domains in hosts. Local networks and redirects are blocked.

## Player commands

Command objects use an action field. Volume takes value from 0 to 1; seek takes milliseconds; play_track takes playlist and entry IDs and needs both playback and library_read; palette takes id; open takes page: lyrics, downloads, services or mods. Open is available only from a mod button and preserves an existing user dialog.

## Limits and settings

The runtime has no filesystem, process execution or access to service credentials. A failing mod is disabled until corrected and enabled again. Each call allows 200,000 operations, 32 call levels, 30 seconds, four HTTPS requests, 16 commands and 256 KB of state. Each network response is limited to 256 KB.

Flat settings become text fields, numeric fields and checkboxes. The optional setting_labels object supplies their labels; nested settings use a JSON editor. Up to 64 mods can be enabled at once.
