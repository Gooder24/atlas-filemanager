# Atlas on Linux (CachyOS + Niri/Wayland) — Port Plan

## Table of Contents

1. [Guiding Principles](#1-guiding-principles)
2. [Phase 0 — Baseline Build & Verification Harness](#2-phase-0--baseline-build--verification-harness)
3. [Phase 1 — Workspace / Cargo.toml Surgery](#3-phase-1--workspace--cargotoml-surgery)
4. [Phase 2 — atlas-model (No Changes Expected)](#4-phase-2--atlas-model-no-changes-expected)
5. [Phase 3 — atlas-core (The Big One)](#5-phase-3--atlas-core-the-big-one)
    - 3.1 Filesystem & Icons
    - 3.2 Watcher
    - 3.3 Recycle Bin
    - 3.4 Mounts / udisks
    - 3.5 Cleanup / Disk Analyzer
    - 3.6 Search
    - 3.7 Vault (WinFsp → FUSE)
    - 3.8 Cloud / Extensions
6. [Phase 4 — atlas-ui (GPUI / Wayland)](#6-phase-4--atlas-ui-gpui--wayland)
7. [Phase 5 — Windows-Only Crates](#7-phase-5--windows-only-crates)
8. [Phase 6 — Niri/Wayland-Specific Integration](#8-phase-6--niriwayland-specific-integration)
9. [Verification Strategy (Per-Crate)](#9-verification-strategy-per-crate)
10. [Per-Crate "Done" Checklist](#10-per-crate-done-checklist)
11. [Testing Matrix for CachyOS + Niri](#11-testing-matrix-for-cachyos--niri)
12. [Risks & Mitigations](#12-risks--mitigations)

---

## 1. Guiding Principles

- Respect the layered architecture described in `ARCHITECTURE.md`:
    ```
    atlas-ui -> atlas-core -> atlas-model
    atlas    -> atlas-ui + atlas-core + atlas-model
    atlas-model -> no runtime crates
    ```
- **Never** let a Windows API leak into `atlas-model`.
- **Every** OS-specific implementation in `atlas-core` must live behind `#[cfg(target_os = "windows")]` or `#[cfg(target_os = "linux")]` — never inline.
- The Linux path is a **first-class target**, not a shim. Prefer FreeDesktop.org standards over hardcoded paths.
- Every change must be verifiable by `cargo check`, `cargo clippy`, and `cargo test` on **both** Windows (CI) and Linux (CachyOS).
- Feature-flag aggressively. A missing feature should be a clean `#[cfg]` gate, not a compile error and not a silent no-op.

---

## 2. Phase 0 — Baseline Build & Verification Harness

**Goal:** Prove the project builds on Linux _as-is_ (mostly fails) and set up the feedback loop.

### 0.1 Install CachyOS build dependencies

```bash
sudo pacman -S --needed base-devel rustup clang pkgconf \
  libxcb wayland wayland-protocols libxkbcommon \
  vulkan-icd-loader vulkan-headers libvulkan \
  openssl fontconfig freetype2 alsa-lib \
  ffmpeg unzip 7zip unrar \
  gtk3 gtk4 libportal
```

`unrar`, `7z`, `ffmpeg`, `pdftoppm` (poppler), `gs` (ghostscript), `pandoc`, `libreoffice`, `rclone`, `ebook-convert` (calibre) are **optional runtime engines** — the app must function without them. They are only probed by `Tools::discover()` in `crates/atlas-core/src/convert/tools.rs`.

### 0.2 Toolchain

```bash
rustup toolchain install 1.97.1 --profile minimal -c rustfmt,clippy
rustup default 1.97.1
```

The repo pins `rust-toolchain.toml` to `1.97.1`, matching the requirement in `Cargo.toml` for `hint::cold_path` used by GPUI.

### 0.3 First baseline build attempt

```bash
cargo check --workspace 2>&1 | tee build_baseline.log
```

Capture the log — every error you see is a task in Phase 1 or 2. Categorize errors into:

- **Missing Windows crate** → Phase 1 (workspace Cargo.toml)
- **Windows-only API in code** → Phase 3/4 (feature gate)
- **Missing Linux crate** → add dependency
- **Wayland/GPUI backend** → Phase 4

### 0.4 Set up a repeatable verification script

Create `scripts/verify-linux.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

CRATE="${1:-atlas-core}"
MODE="${2:-check}"   # check | test | clippy

echo "==> Verifying $CRATE ($MODE)"

case "$MODE" in
  check)  cargo check  -p "$CRATE" --all-features ;;
  test)   RUST_MIN_STACK=16777216 cargo test -p "$CRATE" --all-features --no-fail-fast ;;
  clippy) cargo clippy -p "$CRATE" --all-features --all-targets -- -D warnings ;;
  *) echo "unknown mode: $MODE" >&2; exit 2 ;;
esac

echo "==> OK: $CRATE ($MODE)"
```

`RUST_MIN_STACK=16777216` is mandated by `CONTRIBUTING.md` because the debug translation initializer needs a 16 MiB stack.

---

## 3. Phase 1 — Workspace / Cargo.toml Surgery

**Goal:** Make `cargo check` on Linux reach `atlas-model` and `atlas-core` before touching any code.

### 1.1 Root `Cargo.toml`

Add a `[target.'cfg(windows)'.dependencies]` guard for `atlas-win-space` — it already exists, good. Confirm it's under `[target.'cfg(windows)'.dependencies]`, not `[dependencies]`.

### 1.2 `crates/atlas-core/Cargo.toml`

Current problematic lines:

```toml
[target.'cfg(windows)'.dependencies]
atlas-win-space = { path = "../atlas-win-space" }
winfsp = { version = "0.13.2", default-features = false, features = ["delayload"] }
windows = { version = "0.61", features = [...] }
```

These are already correctly gated. Add a Linux section:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
fuser = "0.15"                     # FUSE for vault mounts
# libc already comes transitively; make it explicit if you call into it
libc = "0.2"
```

Then guard Windows-only module paths in `src/vault.rs`:

```rust
#[cfg(windows)]
pub mod mount;
#[cfg(target_os = "linux")]
pub mod mount_linux;
```

### 1.3 `crates/atlas-ui/Cargo.toml`

The entire `[target.'cfg(windows)'.dependencies]` block is fine — it will simply be skipped on Linux. But `atlas-win-space` is referenced in `Cargo.toml` under Windows deps, and in `src/dnd/windows/*` — those modules must be gated in `lib.rs` / `dnd/mod.rs`.

Verify the `gpui_platform` features already include `x11` and `wayland`:

```toml
gpui_platform = { ..., features = ["font-kit", "x11", "wayland"] }
```

✅ Already correct.

### 1.4 `crates/atlas-win-space` and `crates/atlas-icon-extension`

Remove from `[workspace]` members OR make them conditional. Recommended: keep them in the workspace but only build on Windows:

```toml
[workspace]
members = [
    ".",
    "crates/atlas-model",
    "crates/atlas-core",
    "crates/atlas-ui",
    "crates/atlas-publisher",
    "crates/atlas-cloud-extension",
    "crates/atlas-search-service",
    # Windows-only:
    # "crates/atlas-win-space",
    # "crates/atlas-icon-extension",
]
```

But `atlas-core` and `atlas-ui` reference them, so simply removing them breaks the Windows build. Better approach: keep them as workspace members, but ensure their Cargo.toml files have no Linux host deps and that nothing on the Linux build path pulls them in.

Then in `crates/atlas-win-space/Cargo.toml`, make sure `[target.'cfg(windows)'.dependencies]` is used, and the crate is `#[cfg(windows)]`-gated at the `lib.rs` root with a stubbed no-op API on other platforms — OR rely on the fact that the crate is only linked when `#[cfg(windows)]` blocks reference it. The latter requires that **no** non-Windows code path references it.

**Verification:**

```bash
cargo check -p atlas-model   # must succeed on Linux
```

---

## 4. Phase 2 — atlas-model (No Changes Expected)

`atlas-model` is explicitly designed with no runtime dependencies. Confirm with:

```bash
cargo tree -p atlas-model
```

You should see only `serde`, `serde_json`, `compact_str`, `chrono`, `uuid`, `dirs`, `toml`, `anyhow`. **No** `tokio`, **no** `gpui`, **no** `windows`, **no** `winfsp`.

Review these files for platform-specific assumptions:

- `location.rs` — already has `#[cfg(feature = "provider-host")]` gating ✅
- `file_entry.rs` — uses `std::path`, cross-platform ✅
- `profiles.rs` — uses `dirs::home_dir()`, cross-platform ✅
- `settings.rs` — plain serde ✅

**Action:** None required. This crate is the contract.

**Verification:**

```bash
./scripts/verify-linux.sh atlas-model check
./scripts/verify-linux.sh atlas-model test
./scripts/verify-linux.sh atlas-model clippy
```

---

## 5. Phase 3 — atlas-core (The Big One)

This is where ~90% of the porting work lives. Work module by module, top to bottom. **Do not proceed to Phase 4 until `cargo test -p atlas-core` passes on Linux.**

### 3.1 Filesystem & Icons

Files: `src/fs/list.rs`, `src/fs/icon_loader.rs` (in atlas-ui), `src/fs/preview/mod.rs`, `src/fs/preview/linux_thumb.rs`, `src/fs/desktop_apps.rs`.

**Current state:**

- `list.rs` has a `#[cfg(windows)]` native enumerator and a `#[cfg(not(windows))]` `list_dir_portable` fallback ✅
- `linux_thumb.rs` correctly implements the FreeDesktop Thumbnail spec ✅
- `desktop_apps.rs` correctly parses `.desktop` files ✅

**Tasks:**

1. **Audit `list.rs`:** Confirm `list_dir_portable` is used on Linux, and `entry_for_path` in the non-Windows branch does not `use std::os::windows::fs::MetadataExt` (it uses `#[cfg(windows)]` ✅).
2. **Icon loading:** `atlas-ui`'s `icon_loader.rs` and `thumbs.rs` are Windows-heavy. In `atlas-core`'s `preview/mod.rs`:
    - `is_cloud_placeholder` returns `false` on non-Windows ✅ (already implemented)
    - `thumbnail` path must route through `linux_thumb::freedesktop_thumbnail` first, then fall back to decoding images with `image` crate ✅ (already structured that way in `thumbs.rs`)
3. **Desktop apps:** Add unit tests for `DesktopApp::parse_str` on a real CachyOS `.desktop` fixture (e.g. `/usr/share/applications/org.gnome.Nautilus.desktop`).

**Verification:**

```bash
cargo test -p atlas-core fs:: --all-features
```

Add specific tests:

- Listing `/usr/share/applications` returns `.desktop` files
- `apps_for_file` returns a non-empty list for a `.png`
- `freedesktop_thumbnail` matches the MD5 hash spec

### 3.2 Watcher

File: `src/fs/watcher.rs`

**Current state:** Has a `#[cfg(not(windows))]` branch using `notify` ✅.

**Tasks:**

1. Confirm the `notify` backend on Linux is `inotify`. This works under Niri without extra privileges.
2. Test that recursive watching works. The current implementation uses `watch_path_recursive` in `space_lens.rs`, but `navigate` only uses `watch_path` (non-recursive). Add a Linux integration test that:
    - Creates a temp dir
    - Starts a `watch_path` handle
    - Writes a file, renames it, deletes it
    - Asserts the channel receives `Upsert`, `Rename`, `Remove`

**Verification:**

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core watcher --all-features
```

### 3.3 Recycle Bin

File: `src/fs/recycle_bin.rs`

**Current state:** Fully implements the FreeDesktop Trash spec ✅. The `parse_trashinfo`, `parse_deletion_date`, and `percent_decode` are already cross-platform.

**Tasks:**

1. Verify `accessible_bin_dirs()` returns `~/.local/share/Trash` on CachyOS (or `$XDG_DATA_HOME/Trash`).
2. Integration test:
    - Create a file
    - Send it to trash (`trash::delete`)
    - `list_recycle_bin()` returns it
    - `restore()` restores it
    - `purge()` deletes it

Note: the `trash` crate (v5) is already Linux-compatible, so no shims needed.

**Verification:**

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core recycle_bin --all-features
```

### 3.4 Mounts / udisks

Files: `src/fs/mounts.rs`, `src/fs/udisks.rs`

**Current state:**

- `mounts.rs` uses `sysinfo` for `list_drives` (cross-platform ✅) and `/proc/mounts` for Linux ✅
- `udisks.rs` calls `lsblk --json` and `udisksctl` (both available in CachyOS) ✅

**Tasks:**

1. `udisksctl` requires `udisks2` daemon — verify it's running:
    ```bash
    systemctl status udisks2
    ```
    CachyOS ships it by default. If not: `sudo pacman -S udisks2`.
2. Ensure `lsblk` is available: `pacman -S util-linux` (installed by default).
3. `sysinfo` on Linux reads `/proc` — confirm `Disks::new_with_refreshed_list()` works under Niri (it should).

**Verification:**

```bash
cargo test -p atlas-core mounts --all-features
cargo test -p atlas-core udisks --all-features  # will be no-op on non-Linux CI
```

On the target machine, add a manual test in `docs/` showing a USB device enumeration.

### 3.5 Cleanup / Disk Analyzer

Files: `src/cleanup/`

**Current state:**

- `mod.rs` publishes `ScanJob`
- `mft.rs` is NTFS-only — needs gating
- `walk.rs` is parallel and cross-platform ✅
- `tree.rs`, `policy.rs`, `scan.rs`, `execution.rs` are cross-platform ✅

**Tasks:**

1. In `src/cleanup/mod.rs`:
    ```rust
    #[cfg(windows)]
    mod mft;
    ```
2. In `src/cleanup/scan.rs`, replace the Windows MFT branch with a clean cfg gate:
    ```rust
    #[cfg(windows)]
    if self.snapshot.fast_scan_available && self.try_mft(&root) {
        return;
    }
    self.walk_tree(&root);
    ```
    ✅ Already structured this way.
3. Verify `atlas_win_space::cleanup::*` calls in `execution.rs` are gated. They are currently unconditional in `delete()` and `free()`. Add:
    ```rust
    #[cfg(windows)]
    fn delete(candidate: &CleanupCandidate) -> io::Result<u64> {
        atlas_win_space::cleanup::delete_verified(candidate)
    }
    #[cfg(not(windows))]
    fn delete(_candidate: &CleanupCandidate) -> io::Result<u64> {
        Err(io::Error::other("cleanup_unavailable"))
    }
    ```
4. `caps.rs` / `cleanup::ScanMode::FastFolder` must be rejected on Linux — `try_mft` will never be called because `fast_scan_available` is false on non-Windows.

**Verification:**

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core cleanup --all-features
```

Add integration tests:

- `ScanJob::start(..., ScanMode::Folder)` over a temp tree with known sizes
- Assert `snapshot.bytes` matches `du -sb`
- Assert `partial` status when unreadable subdir present
- Assert `walk.rs` parallel correctness (already tested in `walk.rs` tests ✅)

### 3.6 Search

Files: `src/search/`, `src/fs/index.rs`

**Current state:**

- `catalog.rs` — cross-platform trait ✅
- `nucleo_pool.rs` — cross-platform ✅
- `content.rs` — uses `grep-searcher`, cross-platform ✅
- `service.rs` — has `#[cfg(unix)]` Unix socket path ✅
- `volume_index.rs` — NTFS MFT-specific; only used by `atlas-search-service`
- `name.rs` — uses `ignore::WalkBuilder`, cross-platform ✅

**Tasks:**

1. **Primary backend on Linux:** `fs::index::build_index_scoped` is already called by `goto_index_probe.rs` and `fs/index.rs` — this is the intended Linux path.
2. `search/service.rs::linux_socket_path()` uses `$XDG_RUNTIME_DIR/atlas-search.sock` ✅ — works under Niri (systemd user session sets `XDG_RUNTIME_DIR`).
3. **`atlas-search-service` side:** `linux_daemon.rs` already exists and uses an in-memory `ArenaTree` — this is a complete Linux implementation. **No Windows-specific code in that crate for the Linux path.**
4. Add feature detection: if the search service is unavailable, fall back to `nucleo` in-process index. The `service.rs::is_service_available()` + `ensure_service_started()` already implement this fallback flow.

**Verification:**

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core search --all-features
RUST_MIN_STACK=16777216 cargo test -p atlas-search-service --all-features
```

Add:

- Native index test: create 10k temp files, verify `build_index_scoped` injects them
- Content search test: create files with known strings, verify hits
- Service test: spawn `atlas-search-service --console`, verify Unix socket responds (already present in `arena_index.rs` ✅)

### 3.7 Vault (WinFsp → FUSE)

Files: `src/vault/`, `src/vault/mount.rs`

**This is the largest single effort.** The current `mount.rs` is WinFsp-only.

**Tasks:**

1. **Gate the Windows mount:**

    ```rust
    // src/vault.rs
    #[cfg(windows)]
    pub mod mount;
    #[cfg(target_os = "linux")]
    pub mod mount_linux;
    ```

2. **Implement FUSE mount in `mount_linux.rs`:**
    - Use `fuser = "0.15"` crate
    - Implement `fuser::Filesystem` trait backed by `Session`
    - Mirror the WinFsp callbacks 1:1:
        - `lookup` → `Session::resolve`
        - `getattr` → `Session::entry`
        - `readdir` → `Session::children`
        - `open` / `release` → handle tracking
        - `read` / `write` → `Session::read_range` / `Session::write_at`
        - `create` → `Session::create_entry`
        - `unlink` → `Session::delete_entry`
        - `rename` → `Session::rename_entry`
    - Mount point: `$XDG_RUNTIME_DIR/atlas-vaults/<uuid>/`
    - Reuse all existing `Control`, `Session`, and mutable-block logic — **only the OS-facing trait changes**

3. **Locking:** The `Session` holds an `fs2::FileExt::try_lock_exclusive` on `lock`. This is cross-platform ✅. `Session` has `Send`-safe interior state; verify `fuser::Filesystem` thread-safety requirement (it requires `Send + 'static`, which `Session` provides via `Mutex<Option<Session>>` inside `Shared` — reuse the same pattern from `mount.rs`).

4. **`atlas_core::vault::live::NAMESPACE`:** `"atlas-private-mounts"` — reuse on Linux, pointing under `$XDG_RUNTIME_DIR` instead of `%LOCALAPPDATA%`.

**Verification:**

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core vault --all-features
```

Add Linux-native tests mirroring `vault/tests.rs`:

- `protect → unlock → read_range → write_at → delete_entry` round trip
- Paths through a real FUSE mount (`/dev/fuse` must be readable by the user; CachyOS grants this by default via group `fuse`)
- Confirm no plaintext touches disk (already tested ✅)

**Note:** `/dev/fuse` permissions — if the user is not in `fuse` group:

```bash
sudo usermod -aG fuse $USER
newgrp fuse   # or log out/in
```

Niri doesn't affect `/dev/fuse`.

### 3.8 Cloud / Extensions

Files: `src/cloud/`, `src/ext/`

**Current state:** Both are process-oriented and platform-neutral. The `rclone` binary has a `.exe` suffix on Windows and none on Linux — already handled by `std::env::consts::EXE_SUFFIX` in `installation.rs`.

**Tasks:**

1. Verify `atlas-cloud-extension` builds on Linux — its only deps are `atlas-core` + `anyhow` + `serde_json` + `tokio`. ✅
2. Verify `atlas-core/src/ext/install.rs::ensure_packaged_icon_provider` is Windows-only (it references `.exe` — gate with `#[cfg(windows)]` or verify the sibling detection works on Linux).
3. `rclone` discovery: `installation.rs` uses `env::current_exe()` and looks for `atlas-cloud-extension` (with no `.exe` on Linux) ✅.

**Verification:**

```bash
cargo check -p atlas-cloud-extension
RUST_MIN_STACK=16777216 cargo test -p atlas-core ext --all-features
RUST_MIN_STACK=16777216 cargo test -p atlas-cloud-extension
```

---

## 6. Phase 4 — atlas-ui (GPUI / Wayland)

### 4.1 Confirm the GPUI Wayland backend

`gpui_platform` features already include `wayland` (see `Cargo.toml`). Confirm the actual crate has the backend enabled. If GPUI only does X11 under Wayland via `xwayland`, the app will still work but without native protocol integration. **Test first:**

```bash
WAYLAND_DISPLAY=wayland-1 cargo run --release
```

If it fails with a Wayland error, fall back to `WINIT_UNIX_BACKEND=x11` and file a support issue upstream. Do **not** patch vendored GPUI unless absolutely necessary — the fork in `vendor/gpui-ce` is intentionally pinned.

### 4.2 Gate `shell_thumb`, `shell_menu`, `dnd/windows`, `shell_file_operation`

In `crates/atlas-ui/src/lib.rs`:

```rust
#[cfg(windows)]
pub(crate) mod shell_file_operation;
#[cfg(windows)]
pub(crate) mod shell_menu;
#[cfg(windows)]
pub(crate) mod shell_thumb;
pub(crate) mod dnd;   // dnd/mod.rs already gates its windows submodule
```

Then in `dnd/mod.rs`, the `#[cfg(target_os = "windows")] pub mod windows;` is already correct.

### 4.3 Implement Linux Drag & Drop

File: `src/dnd/mod.rs` — there's already a `mod linux` block with `LinuxDndState`, `LinuxDropPayload`, `LinuxDndEvent`. These are complete _types_, but the actual `wl_data_device` listener isn't wired.

**Tasks:**

1. If GPUI's Wayland backend already supports DnD, only plug its events into `LinuxDndState::handle_drop_uri_list` (present in `dnd/mod.rs` ✅ — accepts `text/uri-list`).
2. If GPUI doesn't support it, implement a minimal `wl_data_device` client via `wayland-client` crate. This is significant work — start with drag _out_ (dragging files from Atlas to other apps) using `text/uri-list`, then drag _in_.

**Note for Niri:** Niri implements DnD through `wl_data_device_v3` correctly. No compositor-specific issues expected.

**Verification:** Manual drag Atlas → Nautilus and back. Automated tests can only cover the pure-DnD logic already unit-tested in `dnd/mod.rs` ✅.

### 4.4 Replace Shell Context Menu

Files: `src/shell_menu/`

Windows-only — gate as above. For Linux, build a context menu from:

- `atlas-core::fs::desktop_apps::apps_for_file(path)` — "Open With"
- Native file operations (cut, copy, paste, rename, trash)
- Custom actions (already in `palette/`)

**Tasks:**

1. Add a Linux branch to whatever builds the context menu spec.
2. `.desktop` `Exec=` templates must be parsed and launched via the existing `build_launch_command` ✅.

### 4.5 Replace Shell Icons

Files: `src/shell_thumb.rs`, `src/icon_theme.rs`, `src/app/icon_loader.rs`

Windows-only — gate `shell_thumb.rs`. Non-Windows icon pipeline (already present ✅):

- `app/icon_loader.rs::rasterize_multi` — parses SVG icon themes via `resvg` ✅
- `ui/thumbs.rs` — decodes image thumbnails via `image` crate ✅
- Fall back to `linux_thumb::freedesktop_thumbnail` for file-type icons from the OS

**Tasks:**

1. In `icon_theme.rs`, ensure `find_vscode_theme` works cross-platform — it uses `dirs::home_dir()` ✅ but hardcodes `.vscode/extensions`. Add:

    ```rust
    if let Some(idx) = scan_ext_dir(&home.join(".local/share/icons")) {
        return Some(idx);
    }
    ```

    `.local/share/icons` is the FreeDesktop icon theme dir — many VS Code icon themes are available here via `papirus-icon-theme` etc.

2. In `src/app/icon_loader.rs`, the `rasterize_multi` function is cross-platform ✅ — `resvg` and `tiny-skia` have no platform-specific code.

3. In `ui/thumbs.rs`, the `request_shell_thumbnail` is `#[cfg(target_os = "windows")]` ✅ — add a corresponding `request_linux_thumbnail` gated on non-Windows that calls `linux_thumb::freedesktop_thumbnail`.

### 4.6 Window Chrome & Niri

**Niri is a scrollable tiling compositor with its own decorations and window rules.** Two things matter:

1. **Do not draw your own title bar.** In `crates/atlas-ui/src/lib.rs`, `window_options.titlebar = Some(TitleBar::title_bar_options())` — this is GPUI's own chrome. It's fine if GPUI is using `xdg-decoration` protocol, but if it draws an in-window title bar, gate it:

    ```rust
    let titlebar = if cfg!(target_os = "linux") {
        None   // Niri handles it
    } else {
        Some(TitleBar::title_bar_options())
    };
    ```

    **Verify** under Niri whether GPUI's chrome integrates or fights the compositor.

2. **Window rules:** Add a `niri` example config or README note:
    ```kdl
    window-rule {
        match app-id="atlas"
        default-column-width { proportion 0.5; }
    }
    ```
    The application ID is set in `WindowOptions { app_id: Some("atlas".to_string()), .. }` ✅.

### 4.7 Clipboard

Files: `src/dnd/windows/ole_clipboard.rs` — Windows-only.
Non-Windows path uses `arboard` (already a dependency ✅). In `app/ops.rs`, the `#[cfg(unix)]` branch sets `text/uri-list` on the clipboard, plus `LinuxClipboardKind::Primary` ✅.

**Tasks:**

1. Verify `arboard` works under Niri. It uses `wl_data_device_manager` — should be transparent.
2. Test: select files in Atlas → Ctrl+C → paste into Nautilus. And the reverse.

### 4.8 Everything Else

Grep the whole `crates/atlas-ui/src` tree for `windows::` (the crate) and `winfsp`. Every hit must be inside a `#[cfg(windows)]` block. Common offenders:

```bash
grep -rn "windows::\|winfsp\|atlas_win_space" crates/atlas-ui/src --include="*.rs" \
  | grep -v "#\[cfg(windows)\]" | grep -v "//"
```

Run this from the repo root and confirm every remaining hit is either inside a cfg block (verify by checking the surrounding lines) or is not actually Windows code (e.g. `std::os::windows::fs::MetadataExt` — which is illegal to have outside a cfg block).

---

## 7. Phase 5 — Windows-Only Crates

### `crates/atlas-win-space`

- Keep as a workspace member (Windows build needs it).
- Add a stub `lib.rs` for non-Windows:
    ```rust
    #![cfg_attr(not(windows), allow(unused))]
    #[cfg(windows)]
    mod crash;
    #[cfg(windows)]
    mod cleanup;
    // ... etc.
    #[cfg(not(windows))]
    pub fn unused_stub() {}
    ```
- Or, better, use `[target.'cfg(windows)'.dependencies]` in every crate that depends on it (already the case for `atlas-core` and `atlas-ui`). The crate is then simply not built on Linux.

### `crates/atlas-icon-extension`

Same treatment. This crate ships a Windows-only binary and reads `.exe` provider manifests.

### `crates/atlas-search-service`

Has a complete `linux_daemon.rs` (Unix socket + ArenaTree index). **No Windows-specific code on the Linux path** — verify with:

```bash
cargo check -p atlas-search-service --target x86_64-unknown-linux-gnu
```

---

## 8. Phase 6 — Niri/Wayland-Specific Integration

| Concern                         | Action                                                                                       |
| ------------------------------- | -------------------------------------------------------------------------------------------- |
| Window decorations              | Do not draw in-window title bar under Wayland                                                |
| App ID                          | Set `app_id: Some("atlas")` — already done ✅                                                |
| Global shortcuts                | Use `org.freedesktop.portal.GlobalShortcuts` or `niri`'s `binds` — likely unnecessary for v1 |
| Native file pickers             | `rfd` uses `xdg-desktop-portal` — verify `xdg-desktop-portal-gtk` or `-gnome` is running     |
| Scale factor                    | GPUI reads `wl_output.scale` — verify UI at 1x, 1.5x, 2x                                     |
| Fractional scaling              | Niri supports it; verify GPUI handles fractional correctly                                   |
| Notification (toasts)           | Currently rendered in-window — fine, no portal needed                                        |
| Autostart                       | User's responsibility, but document `.config/autostart/atlas.desktop`                        |
| File associations (`MimeType=`) | Document `.desktop` packaging                                                                |
| Search service                  | `systemd --user` unit — ship one in `docs/`                                                  |

Example Niri config addition (document in `docs/linux.md`):

```kdl
// ~/.config/niri/config.kdl
window-rule {
    match app-id="atlas"
    default-column-width { proportion 0.6; }
    open-on-workspace "browsing"
}
```

---

## 9. Verification Strategy (Per-Crate)

### General principle

Verification must be **crate-scoped** and **automated** wherever possible. Manual smoke tests are only acceptable for things that fundamentally cannot be automated (DnD gestures, compositor integration).

### Verification commands

For **`atlas-model`** (must pass, full suite):

```bash
./scripts/verify-linux.sh atlas-model check
./scripts/verify-linux.sh atlas-model test
./scripts/verify-linux.sh atlas-model clippy
```

For **`atlas-core`** (must pass, full suite — this is the gate before touching UI):

```bash
./scripts/verify-linux.sh atlas-core check
RUST_MIN_STACK=16777216 cargo test -p atlas-core --all-features --no-fail-fast -- --nocapture
./scripts/verify-linux.sh atlas-core clippy
```

To run **only the core logic tests** (faster inner loop, excludes the `cloud-backend` and other optional features that require an external process):

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core --lib --no-fail-fast
```

To run **only filesystem module tests**:

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core --lib fs:: -- --nocapture
```

To run **only the ported modules** (everything you touched):

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core --lib \
  'fs::' 'search::' 'cleanup::' 'vault::' 'convert::' --no-fail-fast
```

Note: `cargo test` filter accepts multiple substrings since Rust 1.50+ — but only in recent stable versions. If your filter syntax doesn't work, run them one at a time.

### Compile-only verification

To be sure `atlas-core` compiles _without_ running any tests (fastest check, useful after a refactor):

```bash
cargo check -p atlas-core --all-features
cargo check -p atlas-core --no-default-features   # ensure no optional-feature dependency leaks
cargo check -p atlas-core --all-features --target x86_64-unknown-linux-gnu
```

That last line is critical: even on Windows CI, you can **cross-compile check** for Linux to prove the Windows build of `atlas-core` doesn't accidentally reference a Windows-only type outside a cfg gate. Install the target once:

```bash
rustup target add x86_64-unknown-linux-gnu
```

Then:

```bash
cargo check -p atlas-core --all-features --target x86_64-unknown-linux-gnu
```

If this passes on a Windows machine, you've proven the Linux path is at least type-correct. It will _not_ verify runtime behavior, but it catches 80% of porting mistakes before you even boot Linux.

### Integration tests (atlas-core/tests/)

Add these new test files:

- `tests/linux_thumbnail.rs` — asserts FreeDesktop hash and decode
- `tests/linux_watcher.rs` — create/rename/delete round trip
- `tests/linux_recycle.rs` — trash → restore → purge
- `tests/linux_search_native.rs` — 10k-file index + query
- `tests/linux_cleanup.rs` — parallel walker correctness against `du`

Run:

```bash
RUST_MIN_STACK=16777216 cargo test -p atlas-core --tests --no-fail-fast
```

### Feature-matrix verification

`atlas-core` has several features (`free`, `provider-host`, `cloud-backend`, `rar`, `sevenz`). Verify all meaningful combinations on Linux:

```bash
for f in "" "provider-host" "cloud-backend" "provider-host,cloud-backend" \
         "rar" "sevenz" "provider-host,cloud-backend,rar,sevenz"; do
  echo "==> features: [$f]"
  cargo check -p atlas-core --no-default-features --features "$f"
done
```

This catches any feature-gated code that references Linux-incompatible types.

---

## 10. Per-Crate "Done" Checklist

A crate is "done" when **all** of the following are true on a CachyOS + Niri machine:

### `atlas-model`

- [ ] `cargo check -p atlas-model` passes
- [ ] `cargo test -p atlas-model` passes
- [ ] `cargo clippy -p atlas-model -- -D warnings` passes
- [ ] `cargo tree -p atlas-model` shows no `tokio`, `gpui`, `windows`, `winfsp`, `atlas-win-space`

### `atlas-core`

- [ ] `cargo check -p atlas-core --all-features` passes
- [ ] `cargo check -p atlas-core --no-default-features` passes
- [ ] `cargo check -p atlas-core --target x86_64-unknown-linux-gnu` passes (from a Windows host too)
- [ ] `cargo test -p atlas-core --all-features --no-fail-fast` passes
- [ ] `cargo clippy -p atlas-core --all-features --all-targets -- -D warnings` passes
- [ ] Feature-matrix check passes (all combos above)
- [ ] No `grep -rn "atlas_win_space\|winfsp\|windows::" crates/atlas-core/src --include="*.rs"` hits that are outside a `#[cfg(windows)]` block
- [ ] All Linux-only modules (`fs::desktop_apps`, `fs::udisks`, `preview::linux_thumb`, `vault::mount_linux`) have passing unit tests
- [ ] The Linux binary can list `/`, `/home`, and `/usr/share/applications` without error
- [ ] The Linux binary can send a file to trash and restore it

### `atlas-ui`

- [ ] `cargo check -p atlas-ui --all-features` passes on Linux
- [ ] `cargo test -p atlas-ui --lib` passes
- [ ] `grep -rn "windows::\|winfsp\|atlas_win_space\|shell_menu\|shell_thumb" crates/atlas-ui/src --include="*.rs"` returns only hits inside `#[cfg(windows)]` blocks
- [ ] The binary launches a window under Niri
- [ ] A folder can be browsed, a file opened with `xdg-open`, a file renamed and trashed via the UI
- [ ] DnD into/out of Nautilus works
- [ ] Clipboard copy/paste into Nautilus works

### `atlas-search-service`

- [ ] `cargo check -p atlas-search-service` passes
- [ ] `cargo test -p atlas-search-service` passes
- [ ] Binary responds on `$XDG_RUNTIME_DIR/atlas-search.sock` (already tested ✅)

### `atlas-cloud-extension`

- [ ] `cargo check -p atlas-cloud-extension` passes
- [ ] `rclone` is discovered from `PATH` or from the bundled path

### `atlas-win-space`, `atlas-icon-extension`

- [ ] `cargo check -p atlas-win-space --target x86_64-pc-windows-msvc` still passes (Windows-only)
- [ ] They are absent from the Linux build graph (`cargo tree -p atlas-core -e normal --target x86_64-unknown-linux-gnu` must not list them)

---

## 11. Testing Matrix for CachyOS + Niri

Run the following manual scenarios on the target machine:

| Scenario                                | Expected result                                        |
| --------------------------------------- | ------------------------------------------------------ |
| Launch under Niri                       | Window appears, native Wayland                         |
| Browse `/`                              | Lists root dir, no crash                               |
| Browse `/home`                          | Lists home contents                                    |
| Open `~/.config`                        | Hidden dirs visible toggle works                       |
| Open a `.png`                           | Opens in default app via `xdg-open`                    |
| Search for a file via Ctrl+K            | Fuzzy results stream in                                |
| Search in content via goto bar          | `grep`-based results appear                            |
| Create a folder                         | Appears in listing                                     |
| Copy 100 files                          | Progress bar shows, files copied                       |
| Move files across partitions            | Progress + completion                                  |
| Send to Trash                           | Appears in `~/.local/share/Trash/files`                |
| Restore from Trash                      | File back at original path                             |
| Empty Trash                             | Trash cleared                                          |
| Convert PNG → JPG                       | Works if `ffmpeg`/`magick` present                     |
| Open a `.zip`                           | Archive browse mode works                              |
| Open a `.rar`                           | Archive browse mode works if `unrar` present           |
| Create a private folder                 | FUSE mount appears in `$XDG_RUNTIME_DIR/atlas-vaults/` |
| Lock private folder                     | FUSE unmounts, ciphertext remains                      |
| Delete a file                           | Removed from disk (Shift+Del)                          |
| Drag file out to Nautilus               | URI list accepted                                      |
| Drag file in from Nautilus              | Dropped at target folder                               |
| Copy file, paste in Nautilus            | Paste works                                            |
| Copy file from Nautilus, paste in Atlas | Paste works                                            |
| Switch profile                          | Layout and theme switch                                |
| Settings → change theme                 | Repaints correctly                                     |
| Ctrl+, → open Settings                  | Renders without crash                                  |
| Command palette `Ctrl+Shift+P`          | Opens                                                  |
| Restart app                             | Session restored                                       |
| Move window between workspaces          | Layout survives                                        |
| HiDPI at 1.5x                           | Text scales; icons do not blur                         |

---

## 12. Risks & Mitigations

| Risk                                                      | Likelihood | Mitigation                                                                                                                                              |
| --------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GPUI Wayland backend missing required protocol (DnD, IME) | Medium     | Test early; if necessary, contribute upstream to `gpui-ce`. Do not fork just for one protocol.                                                          |
| FUSE mount callback ordering differs from WinFsp          | Medium     | Start with the exact callback table; test with `fd`, `ls`, `cp`, `mv` on a mounted vault. Cross-reference with `virtiofsd` and `sshfs` implementations. |
| `unrar` license                                           | Low        | Already `git`-gated behind the `rar` feature. Document in `NOTICE`.                                                                                     |
| Fractional scaling in GPUI                                | Medium     | Test on 1.5x. If broken, document and file upstream.                                                                                                    |
| Path length (`MAX_PATH`) assumptions                      | Low        | `std::path` handles it; test with deep directories.                                                                                                     |
| `udisksctl` requiring polkit authorization                | Medium     | CachyOS ships `polkit`; test with a real USB. If it fails, use `gio mount` as fallback.                                                                 |
| Missing `.desktop` files on minimal systems               | Low        | Fall back to opening with the system default via `open` crate.                                                                                          |
| `systemd --user` search-service failing                   | Low        | `service.rs::ensure_service_started()` already falls back to in-process `nucleo` ✅.                                                                    |
| Non-free firmware / codecs                                | Low        | `ffmpeg` in CachyOS is built with all codecs.                                                                                                           |

---

## Total Effort Estimate (informal)

| Phase                                           | Effort                      |
| ----------------------------------------------- | --------------------------- |
| Phase 1 (Cargo.toml + cfg gates)                | 1–2 days                    |
| Phase 3.1–3.4 (FS, watcher, trash, mounts)      | 3–5 days                    |
| Phase 3.5 (Cleanup)                             | 1–2 days                    |
| Phase 3.6 (Search)                              | 1 day (mostly already done) |
| Phase 3.7 (Vault FUSE)                          | **1–2 weeks**               |
| Phase 3.8 (Cloud / Ext)                         | 2 days                      |
| Phase 4 (UI: DnD, shell replacement, clipboard) | **1–2 weeks**               |
| Phase 5 (Workspace cleanup)                     | 1 day                       |
| Phase 6 (Niri-specific integration)             | 2–3 days                    |
| Testing & bugfix                                | Ongoing                     |

The single biggest item is **`atlas-core`'s vault FUSE port** — everything else follows the pattern that the codebase already advertises for cross-platform support.
