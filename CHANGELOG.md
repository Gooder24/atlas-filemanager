# Changelog

Write user-facing release notes here. Release headings use
`## [VERSION] - YYYY-MM-DD`; the workflow requires notes for the version it builds.

## [Unreleased]

## [0.1.7] - 2026-11-24

### Fixed

- Changing the custom accent color now also updates the sidebar active highlight and ink, list/table active selection, links, and drag targets with dark/light contrast adjustments.
- File and folder icons in Grid view render sharper without dark edge fringes (smooth Catmull-Rom resampling + straight-edge color bleed for built-in icons and supersampled tier selection for SVG icon themes).
- Windows Store (MSIX): Win+E integration now uses `uap5:StartupTask` with `uap10:Parameters` and falls back cleanly to a session shortcut helper when Windows policy reports `DisabledByPolicy`, so Win+E works immediately without rebooting.
- Windows Store (MSIX): taskbar icons now consistently use the dark-navy rounded Atlas brand icon on both dark and light taskbars.

## [0.1.6] - 2026-11-24

### Fixed

- Windows: Arabic and RTL text now renders in the right visual order with properly joined glyphs on DirectWrite (#16).
- Windows: Win+E integration in the Microsoft Store (MSIX) package now uses an opt-in startup shortcut helper with zero registry writes and no `unvirtualizedResources` capability, restoring File Explorer automatically on uninstall. The direct-download Windows installer also restores Win+E automatically on uninstall.
- Scroll overscroll is disabled by default (#42), with theme and Miller column polish.
- File dates (Details, inspector, tooltips, transfer conflicts) show your local time instead of UTC.
- Removing a cloud account works even when its provider extension cannot start.
- Windows 11 glass windows pause blur while dragging or resizing to eliminate drag lag (#6), and Windows 10 uses the legacy blur material.
- Closing Atlas with the window close button saves tabs, panes and grouping; dragging an unselected file moves it directly.
- Dragging files to other applications copies originals safely, Markdown previews scroll smoothly, native Windows PDF previews work reliably, and theme color picker inputs preserve focus and paste.
- Linux: Wayland/GNOME clipboard copy-paste works reliably without stale entries, Drives lists each disk once (including Fedora Atomic), search index stays current, private folders open as folders, and disk cleanup & Recycle Bin integration are available.
- Store packages declare the Microsoft Desktop VCLibs runtime dependency and updated Windows taskbar icons.

### New

- Preview button next to the address bar in each pane, command-palette sort commands, and remappable Quick Look shortcut.
- Linux: default file manager setting, Nix flake, and stripped release binaries with separate debug symbols.

Thanks to @Zayad20 for the Windows Arabic/RTL DirectWrite fix and @Gooder24 for the Linux contributions in this release.

## [0.1.6-beta.4] - 2026-11-21

### Fixed

- File dates (Details, inspector, tooltips, transfer conflicts) now show your local time instead of UTC.
- Removing a cloud account works even when its provider extension cannot start (for example when an antivirus blocks it).
- Linux: copying and pasting files with other apps works on Wayland, including GNOME, and no longer pastes a stale copy after the source app quits.
- Linux: Drives lists each disk once, including Fedora Atomic; the search index stays current; private folders open as a folder; disk cleanup is available; "Open Recycle Bin" is in the palette and Go to.
- Crash report commands are hidden where they do nothing.

### New

- Linux: a setting to make Atlas the default file manager.
- Linux: Nix flake to build and run Atlas on NixOS; release binaries are stripped with symbols shipped separately.

Thanks to @Gooder24 for the Linux contributions in this release.

## [0.1.6-beta.3] - 2026-11-14

### Fixed

- On Windows 11, dragging or resizing a glass window no longer lags or freezes: the blur is paused while the window moves and returns when you let go.

## [0.1.6-beta.2] - 2026-11-13

### Fixed

- Closing Atlas with the window's close button saves tabs, panes and grouping. Previously only
  Ctrl+Q or the one-minute checkpoint saved them.
- Dragging a file that was not selected yet moves it instead of drawing a selection box.
  In Details, dragging from the other columns still draws the box.

### New

- Each pane has a preview button next to the address bar, so the preview panel no longer
  depends on knowing the Space shortcut.
- Sort by name, date modified, size or type, and reverse the order, from the command palette.
  Running the same sort again flips its direction.

## [0.1.6-beta.1] - 2026-11-12

- Keep Enter, Escape, cursor and slider keys inside the open theme color picker,
  preventing Settings from intercepting a pasted color before confirmation.

### Fixed

- Dragging files to other applications copies the originals and never starts a source-deletion operation.
- Markdown previews scroll in the same direction with smooth and precise scrolling.
- Windows PDF previews use the native renderer even with an older Document Previews extension installed.
- Theme color fields keep keyboard focus, accept pasted hex and HSL/HSLA values, and preserve slider precision.
- Conversion format labels use opaque sectors and contrasting text; the selected format is highlighted immediately.
- Windows 10 glass uses the legacy blur material instead of acrylic to avoid the native window-drag penalty.
- Store packages declare the Microsoft Desktop VCLibs runtime dependency; the Inno installer bundles its desktop runtime.

### Changed

- Quick Look is listed in Commands and the shortcut editor. Space can be changed, removed or restored per profile.

### Validation still required

- The new Discord report of a freeze followed by repeated startup crashes needs the affected user's version and crash report.
- Kaspersky's report (#14) requires vendor review of the exact distributed binaries. This beta does not claim an allowlist approval.

## [0.1.5] - 2026-11-05

### New

- Settings redesigned as a searchable palette, with French and more languages selectable.
- Atlas Ember theme, theme editor improvements and new interface icons.
- Local file conversion with a radial menu, including FLAC, H.264/AAC video and optional Opus.
- Quick actions ring for converting and acting on selected files.
- Atlas Free and Atlas Premium. Premium unlocks with a license key, or in-app through the Microsoft Store in regional-price markets.

### Fixed

- Crash on launch caused by some shell extensions, such as Google Drive for desktop (#1, #3).
- Opening sub-folders on SMB and other cloud connections (#2).
- Typing into the fields of the add-connection dialog.

### Stability

- Crash reports with symbolized stacks that can be sent from the app.
- Recovery from worker failures and poisoned locks instead of closing the app.

## [0.1.0] - 2026-10-11

First public alpha.

### Highlights

- Native, GPU-accelerated file manager for Windows built with Rust and GPUI.
- Grid, List, Details, Miller columns and Tree views; tabs and split panes.
- Command palette, fuzzy go-to, indexed search and previews.
- Local file conversion, archives browsed as folders, tags and profiles.
- Optional first-party extensions (icon themes, thumbnails, Pandoc, Opus, cloud drives, document previews).
- Experimental Linux packaging (Debian/Ubuntu, Fedora/openSUSE, Arch, portable); not yet a supported release.
