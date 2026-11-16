//! Linux behaviour of the ported atlas-core modules against a real filesystem.
//! `cargo test -p atlas-core --test linux_integration`; the FUSE test needs
//! /dev/fuse and fusermount3 and runs with `-- --ignored`.
#![cfg(target_os = "linux")]

use atlas_model::{
    app_state::{BgResult, FsChange},
    cleanup::ScanStatus,
    file_entry::{SortColumn, SortDir},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

/// Fixtures live under the target directory: /tmp is excluded from scans and
/// may be a different filesystem than the isolated trash.
fn fixture() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

/// Upper bound for asynchronous work; tests return as soon as the result arrives.
const DEADLINE: Duration = Duration::from_secs(20);

#[test]
fn watcher_reports_create_rename_and_remove() {
    let dir = fixture();
    let (tx, rx) = flume::unbounded();
    let _watch = atlas_core::fs::watcher::watch_path(dir.path(), tx).unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");

    let mut seen = Vec::new();
    let mut wait_for = |want: &dyn Fn(&FsChange) -> bool| {
        let end = Instant::now() + DEADLINE;
        loop {
            if seen.iter().any(|c| want(c)) {
                return;
            }
            match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
                Ok(BgResult::FsChanges { changes, .. }) => seen.extend(changes),
                Ok(_) => {}
                Err(_) => panic!("watcher event missing"),
            }
        }
    };

    std::fs::write(&a, b"hello").unwrap();
    wait_for(&|c| matches!(c, FsChange::Upsert(e) if e.path() == a));
    std::fs::rename(&a, &b).unwrap();
    wait_for(&|c| match c {
        FsChange::Rename { from, entry } => *from == a && entry.path() == b,
        // A rename split across batches arrives as remove + upsert.
        FsChange::Upsert(e) => e.path() == b,
        _ => false,
    });
    std::fs::remove_file(&b).unwrap();
    wait_for(&|c| matches!(c, FsChange::Remove(p) if *p == b));
}

/// Runs only inside the child process started by `trash_restore_and_purge`,
/// whose XDG_DATA_HOME points at an isolated trash.
#[test]
fn trash_child() {
    let Some(work) = std::env::var_os("ATLAS_TRASH_CHILD_DIR") else {
        return;
    };
    let work = PathBuf::from(work);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let file = work.join("restore-me.txt");
    std::fs::write(&file, b"payload").unwrap();
    trash::delete(&file).unwrap();
    assert!(!file.exists());
    let data = PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap());
    assert!(data.join("Trash/files/restore-me.txt").exists());

    let bin = atlas_core::fs::recycle_bin::read_recycle_bin(&SortColumn::Name, &SortDir::Asc);
    let entry = bin
        .iter()
        .find(|e| &*e.name == "restore-me.txt")
        .expect("listed in trash");
    let info = entry.recycle_info().expect("trash metadata");
    assert_eq!(info.original_path, file);
    rt.block_on(atlas_core::fs::recycle_bin::restore(
        entry.path().to_path_buf(),
        info.i_file.clone(),
        info.original_path.clone(),
    ))
    .unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"payload");

    trash::delete(&file).unwrap();
    let bin = atlas_core::fs::recycle_bin::read_recycle_bin(&SortColumn::Name, &SortDir::Asc);
    assert_eq!(bin.len(), 1);
    trash::os_limited::purge_all(trash::os_limited::list().unwrap()).unwrap();
    assert!(
        atlas_core::fs::recycle_bin::read_recycle_bin(&SortColumn::Name, &SortDir::Asc).is_empty()
    );
    assert!(!data.join("Trash/files/restore-me.txt").exists());
}

#[test]
fn trash_restore_and_purge() {
    let root = fixture();
    let work = root.path().join("work");
    let data = root.path().join("data");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "trash_child", "--nocapture"])
        .env("ATLAS_TRASH_CHILD_DIR", &work)
        .env("XDG_DATA_HOME", &data)
        .status()
        .unwrap();
    assert!(status.success());
}

fn write_tree(root: &Path) -> u64 {
    let mut total = 0;
    for d in 0..5 {
        let dir = root.join(format!("dir{d}")).join("nested");
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..20 {
            let len = 1000 * d + 37 * f + 1;
            std::fs::write(dir.join(format!("f{f}.bin")), vec![7u8; len]).unwrap();
            total += len as u64;
        }
    }
    total
}

#[test]
fn folder_scan_totals_match_the_files_on_disk() {
    let dir = fixture();
    let total = write_tree(dir.path());
    let job = atlas_core::cleanup::ScanJob::start(
        dir.path().to_path_buf(),
        1,
        atlas_core::cleanup::ScanMode::Folder,
    );
    let end = Instant::now() + DEADLINE;
    let snapshot = loop {
        let snapshot = job
            .receiver
            .recv_timeout(end.saturating_duration_since(Instant::now()))
            .expect("scan finished");
        if matches!(
            snapshot.status,
            ScanStatus::Complete | ScanStatus::Partial | ScanStatus::Failed
        ) {
            break snapshot;
        }
    };
    assert_eq!(
        snapshot.status,
        ScanStatus::Complete,
        "{:?}",
        snapshot.message
    );
    assert_eq!(snapshot.bytes, total);
    let (size, free) = snapshot.capacity.expect("filesystem capacity on Linux");
    assert!(size > 0 && free <= size);
}

#[test]
fn native_index_finds_ten_thousand_files() {
    let dir = fixture();
    for d in 0..10 {
        let sub = dir.path().join(format!("d{d}"));
        std::fs::create_dir(&sub).unwrap();
        for f in 0..1000 {
            std::fs::write(sub.join(format!("item-{d}-{f}.txt")), b"").unwrap();
        }
    }
    let mut pool = atlas_core::search::nucleo_pool::NucleoPool::new();
    atlas_core::fs::index::build_index_scoped(dir.path().to_path_buf(), pool.injector());
    let end = Instant::now() + DEADLINE;
    pool.set_pattern("item-7-421");
    loop {
        pool.tick();
        let snapshot = pool.nucleo.snapshot();
        let files = snapshot.item_count();
        if files >= 10_010 && snapshot.matched_item_count() >= 1 {
            let best = snapshot.get_matched_item(0).unwrap();
            assert!(
                best.data.path.ends_with("d7/item-7-421.txt"),
                "{:?}",
                best.data.path
            );
            break;
        }
        assert!(Instant::now() < end, "indexed {files} entries");
        std::thread::yield_now();
    }
}

#[test]
fn content_search_returns_only_matching_files() {
    let dir = fixture();
    for i in 0..50 {
        let body = if i % 10 == 3 {
            "alpha needle-42 omega"
        } else {
            "nothing here"
        };
        std::fs::write(dir.path().join(format!("doc{i}.txt")), body).unwrap();
    }
    let (tx, rx) = flume::unbounded();
    atlas_core::search::content::search_content(
        dir.path().to_path_buf(),
        "needle-42".into(),
        Arc::new(AtomicBool::new(false)),
        move |path| {
            let _ = tx.send(path);
        },
    );
    // The sender is dropped when the walk finishes, which ends the iterator.
    let mut hits: Vec<String> = rx
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    hits.sort();
    assert_eq!(
        hits,
        [
            "doc13.txt",
            "doc23.txt",
            "doc3.txt",
            "doc33.txt",
            "doc43.txt"
        ]
    );
}

#[test]
#[ignore = "needs /dev/fuse and fusermount3"]
fn private_folder_mounts_through_fuse_without_plaintext_on_disk() {
    use atlas_core::vault::{self, mount::Mounted, Control};
    let dir = fixture();
    let folder = dir.path().join("Private records");
    std::fs::create_dir_all(folder.join("Nested")).unwrap();
    std::fs::write(folder.join("personal.txt"), b"confidential-marker-5521").unwrap();
    let profile = uuid::Uuid::new_v4().to_string();
    let password = "a long test passphrase";
    let session = vault::protect(
        &folder,
        password,
        vec![profile.clone()],
        &profile,
        &Control::default(),
    )
    .unwrap();
    let root = session.root().to_path_buf();
    let mount = Mounted::start(session).map_err(|(_, e)| e).unwrap();
    let path = mount.path().to_path_buf();
    assert!(path.starts_with(std::env::var("XDG_RUNTIME_DIR").unwrap_or_default()));

    assert_eq!(
        std::fs::read(path.join("personal.txt")).unwrap(),
        b"confidential-marker-5521"
    );
    std::fs::write(path.join("Nested/new.txt"), b"second-marker-7788").unwrap();
    std::fs::create_dir(path.join("Made here")).unwrap();
    std::fs::rename(
        path.join("Nested/new.txt"),
        path.join("Made here/moved.txt"),
    )
    .unwrap();
    let names: Vec<_> = std::fs::read_dir(&path)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(
        names.contains(&"Made here".to_string()) && names.contains(&"personal.txt".to_string())
    );
    std::fs::remove_dir(path.join("Nested")).unwrap();
    assert!(mount.prepare_lock());
    drop(mount);
    assert!(!path.exists() || std::fs::read_dir(&path).unwrap().next().is_none());

    // Only ciphertext and opaque names remain on disk.
    let leaked = Command::new("grep")
        .args([
            "-rlaE",
            "confidential-marker-5521|second-marker-7788|Made here|moved.txt",
        ])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        leaked.stdout.is_empty(),
        "plaintext found in {}",
        String::from_utf8_lossy(&leaked.stdout)
    );

    let session = vault::unlock(&root, password, &profile).unwrap();
    let dir_id = session
        .resolve(Path::new("Made here/moved.txt"))
        .unwrap()
        .expect("moved file");
    assert_eq!(
        &*session.read(&dir_id, 1024).unwrap(),
        b"second-marker-7788"
    );
}
