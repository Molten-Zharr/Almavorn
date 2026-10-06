use almavorn::{
    errors::AppError,
    model::{ImportedTrack, Mode, PlaylistKind},
    store::{Snapshot, Store},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "almavorn-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn create(store: &mut Store, name: &str, mode: Mode) -> i64 {
    store
        .create_playlist(name, mode)
        .unwrap()
        .after
        .playlists
        .iter()
        .find(|playlist| playlist.name == name)
        .unwrap()
        .id
}

fn tracks(directory: &Directory) -> Vec<ImportedTrack> {
    ["Музыка", "Second"]
        .into_iter()
        .enumerate()
        .map(|(index, title)| ImportedTrack {
            path: directory.0.join(format!("{index}.wav")),
            title: title.into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_ms: 100 + index as u64,
            tags: "large metadata is not part of undo".repeat(100),
        })
        .collect()
}

#[test]
fn scoped_snapshots_restore_references_without_changing_other_modes() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    let order = create(&mut store, "Order", Mode::Order);
    create(&mut store, "Empty order", Mode::Order);
    let chaos = create(&mut store, "Chaos", Mode::Chaos);
    create(&mut store, "Empty chaos", Mode::Chaos);
    let desk = store
        .playlists()
        .unwrap()
        .iter()
        .find(|playlist| playlist.kind == PlaylistKind::SortingDesk)
        .unwrap()
        .id;
    store.add_tracks(order, &tracks(&directory), true).unwrap();
    let library = store.playlists().unwrap();
    let ids: Vec<_> = library
        .iter()
        .find(|p| p.id == order)
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.track.id)
        .collect();
    store.copy_tracks(chaos, &ids, false).unwrap();
    store.copy_tracks(desk, &ids[..1], false).unwrap();
    let library = store.playlists().unwrap();
    for scope in ["order", "chaos", "desk"] {
        assert_eq!(
            store.snapshot(scope).unwrap(),
            Snapshot::from_library(&library, scope)
        );
    }
    let before = store.snapshot("chaos").unwrap();
    assert_eq!(before.playlists.len(), 2);
    let protected = store.snapshot("order").unwrap();
    let desk_before = store.snapshot("desk").unwrap();
    let entry = before
        .playlists
        .iter()
        .find(|p| p.id == chaos)
        .unwrap()
        .entries[0]
        .id;
    store.remove_entry(chaos, entry, false).unwrap();
    let after = store.snapshot("chaos").unwrap();
    store.restore("chaos", &after, &before).unwrap();
    assert_eq!(store.snapshot("chaos").unwrap(), before);
    assert_eq!(store.snapshot("order").unwrap(), protected);
    assert_eq!(store.snapshot("desk").unwrap(), desk_before);
    let error = store.restore("chaos", &after, &before).unwrap_err();
    assert_eq!(
        error.downcast_ref::<AppError>(),
        Some(&AppError::UndoConflict)
    );
}

#[test]
fn shared_titles_remain_protected_after_batched_library_loading() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    let order = create(&mut store, "Order", Mode::Order);
    let chaos = create(&mut store, "Chaos", Mode::Chaos);
    store.add_tracks(order, &tracks(&directory), true).unwrap();
    let id = store
        .playlists()
        .unwrap()
        .iter()
        .find(|p| p.id == order)
        .unwrap()
        .entries[0]
        .track
        .id;
    store.copy_tracks(chaos, &[id], false).unwrap();
    let error = store.rename_track(id, "Blocked", chaos, false).unwrap_err();
    assert_eq!(
        error.downcast_ref::<AppError>(),
        Some(&AppError::SharedTrackProtected)
    );
    store.rename_track(id, "Renamed", chaos, true).unwrap();
    let library = store.playlists().unwrap();
    for playlist in library.iter().filter(|p| p.id == order || p.id == chaos) {
        assert_eq!(playlist.entries[0].track.title, "Renamed");
    }
    let error = store.rename_track(id, "Blocked", order, false).unwrap_err();
    assert_eq!(
        error.downcast_ref::<AppError>(),
        Some(&AppError::TrackRenameProtected)
    );
}

#[test]
fn duplicate_names_report_a_typed_error_and_roll_back() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    create(&mut store, "Playlist", Mode::Chaos);
    let before = store.snapshot("chaos").unwrap();
    let revision = store.revision();
    let error = store
        .create_playlist("PLAYLIST", Mode::Chaos)
        .err()
        .unwrap();
    assert_eq!(
        error.downcast_ref::<AppError>(),
        Some(&AppError::PlaylistNameConflict)
    );
    assert_eq!(store.revision(), revision);
    assert_eq!(store.snapshot("chaos").unwrap(), before);
}
