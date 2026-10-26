# Online music

Open "Music services", "Downloads" or "Mods" in the application menu. Default shortcuts: Ctrl+O, Ctrl+J and Ctrl+M.

Press F9 for song lyrics. Almavorn searches tags and filenames, including reversed artist/title ordering. Choose a recording, refine the query, save or edit its lyrics, export TXT/LRC, or remove the saved copy. Timed lyrics can follow playback.

Search uses nine sources: Genius, LRCLIB, Musixmatch, Yandex Music, lyrics.ovh, AZLyrics, SongLyrics, eLyrics and Letras. Genius takes priority among confident song matches.

Enable "Show lyrics" in the Tracks menu to display the source, title and lyrics beside the playlist. Search follows the playing track. "Find elsewhere" excludes the current source for that song. Open F9 to choose uncertain matches.

Under "Settings → Song lyrics", disable online search or individual sources. All sources are enabled by default. When the sources cannot be reached, automatic search pauses until you press "Search again" or re-enable search. Saved lyrics and file tags remain available offline.

Open "Sources" to see which sites found lyrics or could not complete the search. You can add, replace or remove Genius and Musixmatch keys there. Genius uses public search without a key; Musixmatch requires an API key. Yandex Music uses the account connected under "Music services". Lyrics availability depends on the source, region and account access.

Requests contain the artist, title and filename-derived search terms, followed by a source URL or track ID when fetching lyrics. Audio files, full paths, other files and computer specifications are not sent. Sources see your IP address and client name; authenticated services receive their own key or credentials. The Yandex Music library adds standard client information and a random device identifier during sign-in. JavaScript from lyric pages is not executed.

## Downloads

Open "Downloads" and select "Install / update components". Find a song or add a link, then choose its folder and file name. Downloaded music appears on the sorting desk.

Cancel, retry or edit pending downloads. The queue and history survive restarts; retry starts a fresh download. Removing a history entry keeps its audio file.

For Cobalt, enter an API address and optional key supplied by the server owner, then switch the download source to Cobalt.

## Yandex Music

Install service support, add Yandex Music and choose "Sign in". Open the sign-in page and enter the displayed code. Search for music, open liked songs or a playlist, then select "Play".

Availability depends on the account, subscription and region. This connection uses an unofficial Yandex Music API.

## YouTube Music

Install service support and add YouTube Music. Open "YouTube audio components" to install the download components used for playback. Public music search works without signing in.

For your library, choose "Sign in" and paste headers from your YouTube Music browser session or the contents of browser.json. Follow the [browser authentication guide](https://ytmusicapi.readthedocs.io/en/stable/setup/browser.html). The saved session is also used when opening audio. Some recordings have additional access requirements imposed by YouTube.

## Spotify

Spotify plays on a Spotify Connect device. Premium and a Client ID from [Spotify for Developers](https://developer.spotify.com/dashboard) are required.

1. In Almavorn, open "Settings", "Music services", "Connections" and choose "+ Spotify".
2. Click "Open website". Choose Create app; you can name it Almavorn. Return to the player and click "Next".
3. Click "Copy address". Paste it into Redirect URIs in the app's settings on the website and save. Click "Next" in the player.
4. Copy Client ID from the app's page into Almavorn's single field. Click "Connect": the browser opens sign-in, and the player waits for confirmation automatically.
5. Open Spotify on the desired device. In Almavorn, click "Choose device", then open "My music".

If the browser does not open, use "Open sign-in page" or "Copy link". Later sign-ins reuse the saved setup.

A personal Spotify developer app must allow the accounts that use it. See [Spotify quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes).

Use the player's usual pause, skip, seek and volume controls. Its heading identifies the current source. Connections can be renamed, disabled, removed or signed out. Credentials are stored in the system secret store; unlock it through your desktop environment if sign-in cannot be saved.
