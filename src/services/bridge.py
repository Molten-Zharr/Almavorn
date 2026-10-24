"""Pinned service adapters. JSON on stdin/stdout; credentials never enter argv."""
import json
import sys
import time
import contextlib
import io


def yandex(request):
    from yandex_music import Client
    operation = request["operation"]
    secret = request.get("secret") or {}
    if operation == "device":
        code = Client().request_device_code()
        return {"device_code": code.device_code, "user_code": code.user_code,
                "url": code.verification_url, "expires_in": code.expires_in,
                "interval": code.interval}
    if operation == "poll":
        token = Client().poll_device_token(request["device_code"])
        if not token:
            return {"pending": True}
        return {"secret": {"access_token": token.access_token,
                           "refresh_token": token.refresh_token,
                           "expires_at": int(time.time()) + token.expires_in}}
    if secret.get("refresh_token") and secret.get("expires_at", 0) < time.time() + 60:
        import requests
        from yandex_music._client.device_auth import _DEFAULT_CLIENT_ID, _DEFAULT_CLIENT_SECRET
        response = requests.post("https://oauth.yandex.ru/token", data={
            "grant_type": "refresh_token", "refresh_token": secret["refresh_token"],
            "client_id": _DEFAULT_CLIENT_ID, "client_secret": _DEFAULT_CLIENT_SECRET,
        }, timeout=25)
        response.raise_for_status()
        refreshed = response.json()
        secret.update(refreshed)
        secret["expires_at"] = int(time.time()) + refreshed.get("expires_in", 3600)
    client = Client(secret.get("access_token")).init()

    def track(value):
        return {"id": str(value.track_id), "title": value.title or "",
                "artist": ", ".join(a.name for a in value.artists or []),
                "album": next((a.title for a in value.albums or []), ""),
                "duration_ms": value.duration_ms or 0, "available": bool(value.available)}

    if operation == "search":
        result = client.search(text=request["query"], type_="track", page=request.get("page", 0))
        value = {"tracks": [track(t) for t in result.tracks.results] if result and result.tracks else []}
    elif operation in ("library", "liked"):
        likes = client.users_likes_tracks()
        start = request.get("page", 0) * 50
        # Hydrate only the visible page, not an entire large library.
        ids = [t.track_id for t in (likes.tracks or [])[start:start+50]]
        value = {"tracks": [track(t) for t in client.tracks(ids)] if ids else []}
    elif operation == "playlists":
        rows = client.users_playlists_list() or []
        start = request.get("page", 0) * 50
        value = {"playlists": [{"id": str(p.uid) + ":" + str(p.kind), "name": p.title or "", "count": p.track_count or 0} for p in rows[start:start+50]]}
    elif operation == "playlist":
        user, kind = request["playlist"].split(":", 1)
        playlist = client.users_playlists(kind=int(kind), user_id=int(user))
        start = request.get("page", 0) * 50
        ids = [t.track_id for t in (playlist.tracks or [])[start:start+50]]
        value = {"tracks": [track(t) for t in client.tracks(ids)] if ids else []}
    elif operation == "resolve":
        info = client.tracks_file_info(track_id=request["track_id"], quality="hq", codecs=["mp3"], transport="raw")
        info = info.download_info
        if not info or info.codec != "mp3" or info.transport != "raw":
            raise ValueError("unavailable")
        value = {"url": info.url, "headers": {}}
    elif operation == "validate":
        value = {"name": client.me.account.display_name or client.me.account.login}
    else:
        raise ValueError("unknown_operation")
    if secret:
        value["secret"] = secret
    return value


def youtube(request):
    from ytmusicapi import YTMusic, setup
    operation = request["operation"]
    secret = request.get("secret")
    if operation == "headers":
        raw = request["headers"]
        try:
            secret = json.loads(raw)
        except ValueError:
            secret = json.loads(setup(filepath=None, headers_raw=raw))
        if not isinstance(secret, dict) or not any(k.lower() == "cookie" for k in secret):
            raise ValueError("headers")
        client = YTMusic(auth=secret)
        client.get_library_songs(limit=1)
        return {"secret": secret}
    client = YTMusic(auth=secret, language=request.get("language", "en"))
    if operation == "search":
        start = request.get("page", 0) * 50
        rows = client.search(query=request["query"], filter="songs", limit=start + 50, ignore_spelling=False)[start:start+50]
    elif operation == "library":
        # ytmusicapi owns continuation tokens; request enough for the selected page.
        count = (request.get("page", 0) + 1) * 50
        rows = client.get_library_songs(limit=count, order="recently_added")[count-50:count]
    elif operation == "liked":
        count = (request.get("page", 0) + 1) * 50
        rows = client.get_liked_songs(limit=count)["tracks"][count-50:count]
    elif operation == "playlists":
        count = (request.get("page", 0) + 1) * 50
        rows = client.get_library_playlists(limit=count)[count-50:count]
        return {"playlists": [{"id": p["playlistId"], "name": p.get("title", ""), "count": int(str(p.get("count") or "0")) if str(p.get("count") or "0").isdigit() else 0} for p in rows if p.get("playlistId")]}
    elif operation == "playlist":
        count = (request.get("page", 0) + 1) * 50
        rows = client.get_playlist(playlistId=request["playlist"], limit=count, related=False, suggestions_limit=0)["tracks"][count-50:count]
    elif operation == "validate":
        client.get_library_songs(limit=1)
        return {"name": "YouTube Music"}
    else:
        raise ValueError("unknown_operation")
    result = []
    for row in rows:
        if not row.get("videoId"):
            continue
        seconds = row.get("duration_seconds")
        if seconds is None:
            try:
                seconds = 0
                for part in str(row.get("duration", "0")).split(":"):
                    seconds = seconds * 60 + int(part)
            except ValueError:
                seconds = 0
        result.append({"id": row["videoId"], "title": row.get("title", ""),
                       "artist": ", ".join(a.get("name", "") for a in row.get("artists") or []),
                       "album": (row.get("album") or {}).get("name", ""),
                       "duration_ms": int(seconds) * 1000, "available": row.get("isAvailable", True)})
    return {"tracks": result}


def main():
    request = json.loads(sys.stdin.read(1024 * 1024))
    # SDK diagnostics can include tokens or signed URLs. Keep both output channels private.
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        try:
            value = yandex(request) if request["provider"] == "Yandex" else youtube(request)
            result = {"ok": value}
        except Exception as error:
            name = type(error).__name__.lower()
            status = getattr(getattr(error, "response", None), "status_code", 0)
            if "unauthor" in name or status == 401:
                reason = "auth"
            elif "forbidden" in name or status == 403:
                reason = "access"
            elif status == 429 or "ratelimit" in name:
                reason = "limit"
            elif "timeout" in name or "connection" in name:
                reason = "network"
            elif "modulenotfound" in name:
                reason = "install"
            else:
                reason = "service"
            result = {"error": reason}
    sys.stdout.write(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
