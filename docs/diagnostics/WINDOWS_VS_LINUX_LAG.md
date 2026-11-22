# Por qué el lag de pane splits es peor en Windows que en Linux

**Fecha:** 2026-06-28
**Branch:** gpui-migration
**Método:** 4 subagentes paralelos (render path, referencia Zed, costo por-frame, sistema de animación) + verificación del backend de plataforma de gpui.

Documento complementario de `ANIMATION_LAG_DIAGNOSIS.md` (parcialmente desactualizado — ver §2).

---

## TL;DR

El costo CPU del lag es **idéntico** en Windows y Linux: cada frame de una animación de split re-renderiza el árbol `App` entero, O(N tiles). Lo que hace que **se note más en Windows** es el backend de presentación:

1. **`WindowBackgroundAppearance::Blurred`** (`crates/atlas-ui/src/lib.rs:272`) → ventana transparente con **acrylic blur de DWM**. El compositor de Windows recompone y desenfoca **toda la ventana cada frame presentado**. No puede usar el fast-path de ventana opaca.
2. **`DwmFlush()` bloquea el render thread** (`gpui_windows/src/vsync.rs:40-55`). El pacing a 60 Hz acopla el render al reloj del compositor DWM. Si el frame CPU pesado (re-render O(N)) ya consumió casi los 16.6 ms, no queda holgura → el flush llega tarde → **frame perdido entero** = tirón visible.
3. **DirectX + blend de transparencia**: cada frame sube la escena (`upload_scene_buffers`, `WRITE_DISCARD`) y DWM la blendea contra el escritorio. Más panes = más primitivos = más área a componer con blur.

En Linux (backend `gpui_linux`, Wayland/X11, sin acrylic DWM) el **mismo** trabajo CPU cabe mejor en el presupuesto de frame: la presentación no pasa por un compositor que desenfoca toda la ventana ni por un `DwmFlush` que serializa render ↔ compositor.

**Conclusión:** el lag es el mismo bug de arquitectura en ambas plataformas (re-render del `App` completo por frame de animación), pero Windows lo amplifica por la composición glass/DWM. **Arreglar la raíz (DockHost, §3) reduce el lag en las dos; en Windows el efecto será mayor porque elimina la mayoría de frames pesados que el DWM no alcanza a componer a tiempo.**

---

## 1. La causa raíz (común a ambas plataformas)

`dock_split_shares` (`crates/atlas-ui/src/ui/dock/render.rs:274`) llama:

```rust
window.request_animation_frame();
```

`request_animation_frame()` (gpui `window.rs:2147`) internamente hace `cx.notify(self.current_view())`. Como `dock_split_shares` corre **dentro de `App::render`**, `current_view() == App`. Resultado: cada frame de la animación (~260 ms × 60 fps ≈ 15 frames por split) **notifica al `App` entero** → `App::render` completo → reconstruye el árbol del dock recursivo, O(N tiles): tab-strips, wrappers, event handlers (`Entity::clone`), labels (`format!`/`SharedString`).

Con 30 panes ≈ ~1.5k allocaciones/frame solo de estructura, ×15 frames por split. **El término que escala con N es reconstruir todo el árbol `App` cada frame.**

### Por qué `.cached()` no salva a los panes

Hoy `cx.notify(entity)` solo marca `dirty_views`, **no** pone `refreshing = true` global. Los `FilePanel` cacheados como `Entity` propios **no** re-pintan su contenido pesado. **Pero** el cache key incluye `bounds` (`view.rs:156`): animar `flex_grow` cambia el ancho cada frame → invalida la cache solo de los 2-4 panes que redimensionan (no los 30). El costo de items queda acotado; el costo de reconstruir el árbol `App` es el que escala con N.

---

## 2. Qué del diagnóstico previo (`ANIMATION_LAG_DIAGNOSIS.md`) quedó desactualizado

| Afirmación del doc previo | Estado real (verificado en código actual) |
|---|---|
| `cx.notify()` en `render.rs:275` es el culpable; borrarlo arregla ~100% | ❌ **La línea ya fue borrada.** El lag persiste porque `request_animation_frame()` notifica a `current_view()==App` igual. El fix movió el problema, no lo eliminó. |
| `rect_recorder` O(panes×items) durante split | ❌ **Ya gateado OFF**: `record_rects = dock_split_animations.is_empty() && pane_layout_animations.is_empty()`. No contribuye en animación. |
| `DURATION` 260 ms vs settle 90 ms → 170 ms desperdiciados | ❌ **Falso.** El `split_spring` (ω=22, ζ=0.68, `app.rs:544`) asienta al 2% en ~261 ms, que coincide con `DURATION=260ms`. El 90 ms (`PANE_MS`) es otra animación distinta. |
| `upload_scene_buffers` sincrónico bloqueante, stall 1-2 ms | ❌ Es `WRITE_DISCARD` estándar (el driver entrega buffer nuevo, sin stall GPU). `Present(0)` sin vsync wait; el pacing va en hilo aparte. **No es cuello de botella.** |
| `Entity<DockAnimator>` separado = fix definitivo | ✅ Correcto y es la única solución de raíz (ver §3). |
| `Transition::new()` view-scoped (Paso 4) | ⚠️ **Inútil por sí solo.** `Transition::apply` es wrapper de `with_animation`; en el render-path de `App` notifica a `App` igual. Solo sirve si el subárbol vive en su propia `Entity`. |

**Costo nuevo no listado antes:** la invalidación de cache por **cambio de `bounds`** (`view.rs:156`) es lo que rompe `.cached()` en los panes que resizean — no un `refreshing` global como asumía el doc.

---

## 3. El fix de raíz: `DockHost` (patrón `PaletteHost`)

El repo **ya tiene el patrón exacto**: `crates/atlas-ui/src/ui/palette_host.rs`, cuyo doc-comment describe literalmente este mismo bug. Se trata de hospedar el subárbol animado en una `Entity` dedicada para que el `request_animation_frame` caiga sobre *ese* view y no sobre `App`.

```rust
// Bosquejo — clonar PaletteHost
struct DockHost { app: WeakEntity<App>, is_rendering: bool }

impl Render for DockHost {
    fn render(&mut self, window, cx) -> impl IntoElement {
        self.is_rendering = true;
        let el = self.app.update(cx, |app, cx| app.render_custom_dock(window, cx));
        self.is_rendering = false;
        el
    }
}
```

- `root.rs:302` renderiza la `Entity<DockHost>` en vez de llamar `render_custom_dock` inline.
- `cx.observe(&app)` con guard `is_rendering` para repintar en cambios estructurales (split/close).
- **El estado (`dock_split_animations`, `app.rs:153-172`) se queda en `App`.** Solo se mueve el *hosting del render*, no el estado (el doc previo se equivocaba al pedir mover todo).

**Efecto:** durante la animación, `current_view() == DockHost` → solo `DockHost` re-renderiza. El chrome del `App` (sidebar, toolbar, statusbar) y los `FilePanel` cacheados quedan intactos. Corta el término lineal-en-N.

**Por qué ayuda especialmente en Windows:** menos frames pesados → más frames caben dentro del presupuesto de 16.6 ms → menos `DwmFlush` que llegan tarde → menos frames que el compositor DWM no alcanza a componer con blur. La diferencia Windows/Linux se estrecha al reducir el número de frames pesados.

**Riesgo a validar:** `sync_pane_focus` corre dentro de `render_custom_dock`; verificar que la cadena `sync_overlay_focus` (el foco se reasigna cada frame) sigue intacta al moverlo bajo `app.update`.

---

## 4. Cómo lo evita Zed (referencia)

Zed no sufre esto porque (`zed-reference/crates/workspace/src/pane_group.rs`):

- **`Entity<Pane>` por panel** (`Member::Pane(Entity<Pane>)`) — cada pane su propio dirty-flag.
- **`.cached()` en cada `Member::Pane`** (`pane_group.rs:552`) — re-render del Workspace solo recoloca, no re-renderiza contenido intacto.
- **Resize fuera del grafo de entities:** muta `flexes: Arc<Mutex<Vec<f32>>>` por eje + `window.refresh()`, **sin `cx.notify()`** a ningún pane (`pane_group.rs:1248`).
- **Element IDs estables** (`basis`) → la cache y el estado de arrastre sobreviven entre frames.
- **Zed NO anima splits** (cero `with_animation`/`ease`/`lerp` en `pane_group`/`pane`). Atlas es más ambicioso → choca con su modelo de un solo `Entity<App>`.

El óptimo a largo plazo sería replicar (a) `Entity` por contenedor `Linear` + (b) `.cached()` + (c) resize por flex-mutate sin notify. `DockHost` es el 80/20 que ataca la raíz sin esa refactorización invasiva.

---

## 5. Plan priorizado

| # | Fix | Archivo | Esfuerzo | Impacto |
|---|-----|---------|----------|---------|
| **1** | `DockHost` (clonar `PaletteHost`) | `ui/dock_host.rs` nuevo, `root.rs:302` | Medio (código probado) | **Crítico — raíz** |
| **2** | `DURATION` 260→~120 ms (propina) | `app.rs:429` | 1 línea (ojo: `PaneAnimation::TTL` también 260) | Bajo-medio |
| **3** | (futuro) `Entity` por `Linear`, estilo Zed | múltiples | Grande/invasivo | Definitivo |

DirectX, present, vsync y rect_recorder **ya no son cuellos de botella**: la diferencia Windows/Linux es consecuencia de que el compositor glass/DWM amplifica cada frame pesado, no de un defecto del renderer en sí.

---

## Archivos clave

- `crates/atlas-ui/src/lib.rs:272` — `WindowBackgroundAppearance::Blurred` (glass activo en Windows)
- `crates/atlas-ui/src/ui/dock/render.rs:274` — `request_animation_frame()` (raíz)
- `crates/atlas-ui/src/ui/root.rs:302` — `render_custom_dock` inline en `App::render`
- `crates/atlas-ui/src/ui/palette_host.rs` — patrón a clonar para `DockHost`
- `crates/atlas-ui/src/app.rs:153-172,429,544` — estado de animación, `DURATION`, `split_spring`
- `zed-reference/crates/gpui_windows/src/vsync.rs:40-55` — `DwmFlush()` que acopla render ↔ compositor
- `zed-reference/crates/gpui/src/window.rs:2147` — `request_animation_frame` → `cx.notify(current_view())`
- `zed-reference/crates/gpui/src/view.rs:156` — cache key incluye `bounds`
- `zed-reference/crates/workspace/src/pane_group.rs:552,1248` — `.cached()` por pane + resize sin notify
