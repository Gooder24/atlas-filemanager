# Plan de saneamiento y completitud — Atlas

Auditoría 2026-07-02. 24 hallazgos en 5 categorías; plan en 4 fases.

## Diagnóstico

### A. DRY — duplicación en vistas
- `grid.rs`, `list.rs`, `details.rs`, `miller.rs` duplican ~150 líneas cada una:
  on_click (select/activate + commit_rename), right-click → ctx menu, bloque
  drop-target de carpeta (`DraggedFiles` + `ExternalPaths`, 4 copias), begin_rubber,
  input de rename embebido. Ya divergieron:
  - grid gatea handlers con `!is_animating`; list/miller/details no → clicks fantasma durante animación.
  - list solo permite drag desde filas seleccionadas; grid siempre.
  - list hardcodea `w(px(220.))`.
- Shims era-migración vivos: `atlas-ui/src/{state/mod.rs, fs.rs, cloud.rs, ext.rs}`
  re-exportan atlas-model/core; conviven rutas `crate::state::X` y `atlas_model::X`.

### B. Arquitectura
- `App` god-object: 126 campos planos (caches UI, scrolls ×5, tag-picker ×4, canales).
- Migración `Location` a medias (Fase 1/6): `Action::Navigate{PathBuf}` coexiste con
  `NavigateLocation{Location}`; Paste/Move/Delete siguen PathBuf; sentinels
  `PathBuf::from("tag:id")`.
- Errores stringly-typed: `Result<(), String>` en `BgResult`, errores `join("\n")`.
  No se distingue cancelado / parcial / fallo.
- i18n rota en core: español hardcoded en atlas-core (11 sitios), 0 usos de rust_i18n.
- `#![allow(dead_code)]` a nivel archivo (input/action.rs) esconde código muerto.

### C. Testing / gates
- Resuelto: `cargo test --workspace` compila tras retirar Extism y las crates WASM.
- ~100 lints clippy preexistentes en atlas-ui. Sin CI.

### D. Higiene repo
- Raíz con ~25 archivos basura (scripts python one-off, logs, dhat-heap.json,
  test.zip/rar, nul, scratch .rs). `atlas-ui/src/test_layout.rs` huérfano.
- Cargo.toml raíz duplica bloque de lints en vez de `lints.workspace = true`.
- Patches de UI ahora viven en `vendor/` (`gpui-component-fork`,
  `zed-reference`); `taffy-patch` quedó desactivado.

### E. Funcionalidad esperada de un file manager — huecos
| Falta | Estado |
|---|---|
| Comprimir selección (crear zip) | Backends solo leen/extraen |
| Recycle Bin: Restore / Empty | Solo listado |
| DnD Linux/macOS | Solo Windows OLE |
| Propiedades cross-platform | Solo menú shell Windows |
| Preferencias de vista por carpeta | No existe |
| Cloud transfers panel | Pendiente |
| Open With propio | Solo nativo Windows |

## Fases

### F0 — Gates (desbloquea todo) ✅ 2026-07-02
1. [x] `cargo test --workspace` verde (70 pass): `test/doctest = false` en crates wasm.
   Gate: `scripts/check.ps1` (check + test; clippy filtrado aparte por deuda).
2. [x] Raíz limpia: diagnósticos → `docs/`, scratch → `scripts/`, archives → `test_arc/`,
   16 logs/artefactos borrados, `test_layout.rs` eliminado, `.gitignore` ampliado.
   Pendiente decisión usuario: `ai-website-cloner-template/` (ignorado, no borrado).
3. [x] `lints.workspace = true` en paquete raíz.

### F1 — DRY vistas ✅ 2026-07-02
4. [x] Extraídos a `views/mod.rs`: `on_entry_click()`, `on_entry_ctx_menu()`,
   `folder_drop_target()`; consumidos por grid/list/details (+miller el drop).
   ~230 líneas duplicadas eliminadas. `rename_slot` no se extrajo (layout difiere
   por vista). Gate `is_animating` sigue solo en grid: es la única vista con
   `AnimatedVirtualGrid`; unificarlo requiere plumbing que hoy no existe.
5. [x] Shims borrados (`state`, `fs`, `cloud`, `ext`, `search`, `util`);
   ~400 rutas migradas a `atlas_model::` / `atlas_core::`. Gate verde (70 tests).

### F2 — Arquitectura (6, 7, 8, 10 ✅ 2026-07-02)
6. [x] Location: las fases 2-6 ya estaban hechas en la rama (memoria desactualizada).
   Restos saldados: `Navigate{PathBuf}` documentado (semántica remote-aware, no
   duplicado); breadcrumb de Tag/RecycleBin ya no pinta el sentinel `tag:N` ni
   navega a rutas inexistentes (crumb estático + nombre real del tag);
   menú "recent" resuelve labels vía `resolved_location_label`.
7. [x] Errores tipados: `OpErrorKind` (14 variantes) en atlas-model; `OpComplete`
   lleva `Result<(), Vec<OpError>>`; `FilenameError` tipado; `fatal: Option<PathBuf>`.
   UI traduce en `app/ops.rs::op_error_text` / `filename_error_text`.
8. [x] i18n: 0 strings de usuario hardcoded en core (19 claves nuevas en en/es.yml;
   Display de `ArchiveError`/preview → inglés, convención log-only).
9. [ ] `App`: agrupar campos en sub-structs. Re-evaluado: tras borrar los campos
   muertos es cosmético; prioridad baja, hacer oportunista.
10. [x] Sweep dead_code en atlas-ui (allows de archivo fuera):
   - **Bug arreglado**: bg.rs descartaba `ExtRegistryFetched/RemoteInstalled/LocalInstalled`
     → el marketplace nunca reflejaba resultados; ahora cablea los handlers de ext.rs.
   - Borrado: `dock_glue.rs` (huérfano sin compilar), 7 campos legacy de `App`
     (`dock`, `dock_revision`, `pending_dock_ops`, `reconcile_pending`,
     `nucleo_results`, `rename_input`, `pending_preview`), enum `DockOp`,
     maquinaria FLIP vieja (sustituida por el patch paint-offset de gpui),
     8 variantes muertas de `Action` + arms, `NavKind::Replace`, 3 métodos MRU
     muertos de pane_ops, `process_action`.
   - Anotado (aparcado, no muerto): `ToggleAnalyzer` (overlay sin entrada — F3),
     stagger `pane_item_appear_seq`, variantes cfg-Linux del enum `Action`.
   - Los `#![allow(dead_code)]` de atlas-model/core quedan (razón documentada:
     API retenida para fases pendientes).

### F3 — Features (orden valor/esfuerzo) — 11, 12 ✅ 2026-07-02
11. [x] Recycle Bin: `restore_items`/`purge_items`/`empty_bins` en core (errores
    tipados + OpComplete); Actions Restore/Purge/Empty; palette gated por
    Location::RecycleBin; tecla Delete en la papelera purga el par $R/$I
    (antes dejaba $I huérfanos); ctx-menu Restore re-enrutado al motor nuevo
    (el viejo tragaba errores y solo refrescaba el tab enfocado).
12. [x] Comprimir a zip: `fs/compress.rs` (recursivo, progreso, cancel, deflate,
    test incluido); `Action::CompressToZip`; comando de palette con selección.
    Pendiente menor: entrada en el ctx-menu nativo.
13. [x] Cloud transfers panel: `ui/overlays/transfers_bar.rs` (barra bottom-left,
    gateada `cfg(feature = "cloud")`); consume `state.cloud.transfers` +
    `show_transfers` (ya poblados en `app/cloud.rs::TransferUpdate`); reusa
    `CloudAction::{Pause,Resume,Cancel,Retry}Transfer` ya cableados, 0 código
    nuevo en model/core.
14. [x] Per-folder view prefs: `atlas_model::profiles::{FolderViewPref, FolderViewPrefs,
    FolderViewFeature}` (LRU cap 200, MRU-first `Vec`), scoped vía `ProfileScoped`
    como `PreviewFeature`. Restaura en `app/nav.rs::navigate_to` (antes de
    `spawn_list`, para que lea sort_col/dir ya actualizados); guarda en
    `Action::SetSort`/`Action::SetViewMode` (actions.rs) vía helper
    `App::save_folder_view_pref`. Solo aplica a `Location::Directory` (Tag/Archive/
    Remote/RecycleBin no persisten vista).
15. [x] Propiedades cross-platform: `ui/overlays/properties.rs` (nuevo) — overlay
    propio (stat síncrono vía `std::fs::metadata`/`symlink_metadata`: tamaño,
    creado, modificado, read-only, symlink) gateado `cfg(not(windows))` en el
    ctx-menu (`shell_context_menu.rs::AtlasCmd::Properties`); Windows conserva el
    diálogo nativo (`ShellExecuteExW` + verbo "properties") sin tocar. Estado
    `OverlayState::properties: Option<PropertiesState>` en atlas-model; claim de
    foco reutiliza `TextlessModal` (como settings/transfer_conflict). Pendiente
    menor: tamaño recursivo de carpetas (hoy muestra "—", requeriría scan async).
16. [ ] DnD Linux (XDND/wayland) — al final.

### F4 — Deuda de build ✅ 2026-07-04
17. [x] `taffy-patch` desactivado; `taffy` vuelve a crates.io. Forks de
    `gpui`/`gpui-component` vendorized en `vendor/` y patches reapuntados ahí.
    Gate `cargo check --workspace` verde.
