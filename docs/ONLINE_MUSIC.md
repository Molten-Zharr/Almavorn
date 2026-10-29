# Online music

Account connections are under "Settings → Music services → Connections". Lyrics providers have their own "Song lyrics" settings section.

"Music services", "Downloads" and "Mods" are also available under "More…". Default shortcuts: Ctrl+O, Ctrl+J and Ctrl+M.

## Yandex Music

Install service support, add Yandex Music and choose "Sign in". Open the sign-in page and enter the displayed code. Search for music, open liked songs or a playlist, then select "Play".

Availability depends on the account, subscription and region. This connection uses an unofficial Yandex Music API.

## Spotify

Spotify plays on a Spotify Connect device. Premium and a Client ID from [Spotify for Developers](https://developer.spotify.com/dashboard) are required.

1. In Almavorn, open "Settings", "Music services", "Connections" and choose "+ Spotify".
2. Click "Open website". Choose Create app; you can name it Almavorn. Return to the player and click "Next".
3. Click "Copy address". Paste it into Redirect URIs in the app's settings on the website and save. Click "Next" in the player.
4. Copy Client ID from the app's page into Almavorn's single field. Click "Connect": the browser opens sign-in, and the player waits for confirmation automatically.
5. Open Spotify on the desired device. In Almavorn, click "Choose device", then open "My music".

If the browser does not open, use "Open sign-in page" or "Copy link". Later sign-ins reuse the saved setup.

A personal Spotify developer app must allow the accounts that use it. See [Spotify quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes).

## YouTube Music

Install service support and add YouTube Music. Open "YouTube audio components" to install the download components used for playback. Public music search works without signing in.

For your library, choose "Sign in" and paste headers from your YouTube Music browser session or the contents of browser.json. Follow the [browser authentication guide](https://ytmusicapi.readthedocs.io/en/stable/setup/browser.html). The saved session is also used when opening audio. Some recordings have additional access requirements imposed by YouTube.

## Song lyrics

Open "Settings → Song lyrics" for search options, sources, Genius and Musixmatch keys, and the Yandex Music account shortcut. The lyrics panel also has a "Settings" button.

### Find and choose

Enable "Show lyrics" in the Tracks menu or press F9 for the full window. Almavorn searches tags and filenames. Confident matches open automatically; other recordings can be chosen directly in the panel. "Find elsewhere" excludes the current source for that song. Use "Search" in the full window to refine the query.

### Save for the next listen

Click "Save lyrics" in the panel or full window. The copy is stored in the player's local database, linked to the track. On the next listen, including after a restart, saved lyrics open first without API requests. The audio file stays unchanged.

"Delete lyrics" appears for saved lyrics and asks for confirmation. It removes the local copy. "Search again" and "Find elsewhere" explicitly start a new search.

The full window also offers editing and TXT/LRC export. Timed lyrics follow the current line. Word underlining requires actual word timestamps.

### Choose sources

Sources include Genius, LRCLIB, Musixmatch, Yandex Music, lyrics.ovh, AZLyrics, SongLyrics, eLyrics and Letras. All are enabled by default; Genius takes priority among confident matches. Disable individual sources in lyrics settings.

Genius works without a key. Musixmatch requires an API key. Lyrics settings include buttons to enter, replace, obtain and delete keys. Yandex Music uses the account connected under "Music services". Lyrics availability depends on the source, region and account access.

"Sources" in the full window shows search results and errors. When sources cannot be reached, automatic search pauses until "Search again" or re-enabling search. Saved lyrics and file tags remain available offline.

## Downloads

Open "Downloads" and find a song or add a link. The player installs missing components before the first search or download. You can close the window or cancel while it prepares. Downloaded music appears on the sorting desk.

Choose a folder and, optionally, a file name. The player creates missing folders. If the chosen folder is unavailable, it tries "Downloads/Almavorn", then "Downloads" itself. The actual folder appears above the selected history entry. Leave the folder field empty to download directly into "Downloads".

If preparation fails, check your connection, select "Install / update components", then retry the download. Automatic installation pauses for a minute after a failure; the button retries immediately.

Cancel, retry or edit pending downloads. The queue and history survive restarts; retry starts a fresh download. Removing a history entry keeps its audio file.

For Cobalt, enter an API address and optional key supplied by the server owner, then switch the download source to Cobalt.

In Download settings, "API address guide" and "API key guide" open Cobalt's official guides in your browser. Cobalt has no public API. For your own server, follow the [setup guide](https://github.com/imputnet/cobalt/blob/main/docs/run-an-instance.md) and [API key instructions](https://github.com/imputnet/cobalt/blob/main/docs/protect-an-instance.md#configure-api-keys).

## Controls

Use the player's usual pause, skip, seek and volume controls. Its heading identifies the current source. Connections can be renamed, disabled, removed or signed out. Credentials are stored in the system secret store; unlock it through your desktop environment if sign-in cannot be saved.

## Data and access

Requests contain the artist, title and filename-derived search terms, followed by a source URL or track ID when fetching lyrics. Audio files, full paths, other files and computer specifications are not sent. Sources see your IP address and client name; authenticated services receive their own key or credentials. The Yandex Music library adds standard client information and a random device identifier during sign-in. JavaScript from lyric pages is not executed.
