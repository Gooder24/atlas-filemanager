# Diagnóstico: Animation Lag en Windows — Atlas File Manager

**Fecha:** 2026-06-28  
**Branch:** gpui-migration  
**Método:** 6 subagentes paralelos analizando GPUI internals, Windows platform, Zed reference, y código Atlas.

---

## TL;DR

Una sola línea en `dock/render.rs:275` causa que el **App entity completo se re-renderice a 60fps** durante cada animación de split, invalidando el cache de todos los N panes simultáneamente. Combinado con `rect_recorder` O(panes × items) y el pipeline DirectX sincrónico, produce el lag observable.

```rust
// dock/render.rs:274-275 — causa del re-render completo
window.request_animation_frame(); // ✓ correcto
cx.notify();                       // ✗ re-renderiza el App completo
```

---

## Causa Raíz #1 — `cx.notify()` en App durante render path

**Archivo:** `atlas-ui/src/ui/dock/render.rs:274-275`  
**Función:** `dock_split_shares()`

### Qué pasa

`dock_split_shares()` se llama desde `render_tree()` durante el render del `App` entity. Dentro hace:

```rust
window.request_animation_frame();
cx.notify(); // cx: &mut Context<App>
```

`cx.notify()` en `Context<App>` desencadena:

```
cx.notify() en App entity
  → mark_view_dirty(App) → marca App + todos sus ancestros dirty
  → window.refreshing = true
  → cache de TODAS las vistas hijo invalidado (view.rs:160)
  → App::render() completo desde cero
  → render_custom_dock() → render_tree() recursivo sobre N nodos
  → get_or_make_file_panel() × N panes
  → Taffy.clear() + reconstrucción completa del layout tree
  → se repite cada frame (60fps × 260ms = ~15 frames por split)
```

### Por qué es redundante

`request_animation_frame()` internamente hace:
```rust
// window.rs:2147-2149
pub fn request_animation_frame(&self) {
    let entity = self.current_view();
    self.on_next_frame(move |_, cx| cx.notify(entity));
}
```

Ya agenda el re-render para el próximo frame. El `cx.notify()` adicional es completamente redundante y destructivo.

### Escala con múltiples splits

Con N Linear containers con animaciones activas, `dock_split_shares()` se llama N veces por frame → N llamadas a `cx.notify()` en App → aunque dedupliquen, `window.refreshing = true` igualmente.

### Evidencia de conocimiento previo

`root.rs:204-207` tiene este comentario que prueba que el problema ya fue identificado parcialmente:
```
// Note: no App-level `request_animation_frame` here. Each animating `FilePanel`
// self-drives its own frames via `with_animation`, so the whole App tree no
// longer re-renders every animation frame (was the stutter source under many
// panes). FLIP starts re-trigger a frame from the pane's `on_prepaint`.
```

El fix se aplicó a `FilePanel` pero **nunca a `dock_split_shares`**.

---

## Causa Raíz #2 — `rect_recorder` on_prepaint es O(panes × items)

**Archivos:** `atlas-ui/src/views/list.rs:104`, `atlas-ui/src/views/grid.rs:157`

Cada fila visible registra su bounding box vía `on_prepaint`:
```rust
// por cada fila visible en cada pane
view.update(cx, |this, _cx| { this.rect_bounds.insert(id, bounds); })
```

Con 6 panes × 100 ítems visibles = **600 `view.update(cx)` por frame**, cada uno ejecutando:
- `Arc::clone`
- `RefCell::borrow_mut`
- `HashMap::insert`

Escala cuadráticamente al agregar panes. Durante animaciones de split (cuando más panes existen simultáneamente), este costo se multiplica.

**Nota:** `is_animating` (detectado via `dock_split_animations` activos) ya deshabilita `record_rects` en grid/list — pero no el on_prepaint propio.

---

## Causa Raíz #3 — `upload_scene_buffers` sincrónico en DirectX

**Archivo:** `gpui/src/platform/windows/directx_renderer.rs:402-453` (upstream GPUI)

`D3D11_MAP_WRITE_DISCARD` + `memcpy` + `Unmap` para cada tipo de primitiva (shadows, quads, underlines, mono sprites, subpixel sprites, poly sprites) — **bloqueante y sincrónico**. Sin staging buffer async ni double-buffering en CPU side.

Con más panes → más primitivos en escena → buffer más grande → más tiempo en Map/Unmap → stall de 1-2ms por frame que se suma al costo del render.

**Este es upstream GPUI**, difícil de cambiar desde Atlas.

---

## Causas Secundarias

### `DURATION` 260ms vs spring settle 90ms
`DockSplitAnimation::DURATION = 260ms` pero el spring se estabiliza en `PANE_MS = 90ms`. El render loop activo corre 170ms innecesariamente después de que la animación visual terminó.

### `.scale()` en pane body (`render.rs:249-250`)
```rust
.scale((0.48 + 0.52 * visible).clamp(0.48, 1.10))
```
En GPUI/Windows, `.scale()` embebe la matriz 2×2 en **cada primitivo** del subárbol (no es un layer separado). Aumenta el volumen del scene buffer por pane.

### `on_prepaint` update de App por container (`render.rs:178-184`)
Cada `DockLinear` hace `view.update(cx, |this, _cx| { this.dock_linear_bounds.insert(id, bounds) })` — borrow mutable de App por contenedor por frame. Overhead menor pero presente.

### Notificaciones redundantes adicionales
`tick_tab_close()` en `render.rs:65` puede llamar `cx.notify()` dentro del render path si hay tab close animation activa. 16 llamadas totales a `cx.notify()` dentro de `render_custom_dock()`.

---

## Comparación con Zed

**Zed no tiene animaciones de split.** `pane_group.rs` — cero menciones de `animation`, `ease`, `lerp`. Solo `window.refresh()` desde event handlers, nunca desde render path.

**Diferencia arquitectural:**
```
Zed:  Entity<Pane> × N  →  solo el pane afectado re-renderiza
Atlas: Entity<AtlasApp>  →  todo el App re-renderiza cuando App es notificado
```

**Patrón correcto en Zed/GPUI:** `cx.notify()` solo desde event handlers, nunca desde render. Las animaciones se manejan con `with_animation` que notifica solo la view actual.

**Patrón correcto ya existe en el repo** — `sidebar/mod.rs:540` en gpui-component-fork:
```rust
Transition::new(duration).width(from, to).apply(element, id)
```
Delega al sistema de animación de GPUI → solo el elemento animado re-renderiza.

---

## Plan de Fix

### Paso 1 — Borrar la línea culpable (1 línea, máximo impacto)

```rust
// dock/render.rs:274-275
window.request_animation_frame();
// cx.notify(); ← ELIMINAR
```

Elimina el re-render completo de App durante animaciones de split. El `request_animation_frame()` que queda ya maneja el scheduling correcto.

**Impacto esperado:** Elimina ~100% del lag de animaciones de split con múltiples panes.

### Paso 2 — Reducir DURATION (1 línea)

```rust
// Donde esté definido DockSplitAnimation::DURATION o PANE_MS
const DURATION: Duration = Duration::from_millis(90); // era 260ms
```

Reduce el período del render loop a lo que realmente dura la animación visual.

### Paso 3 — Deshabilitar rect_recorder durante split animations

Durante `dock_split_animations.is_empty() == false`, skipear el `on_prepaint` de rect_recorder en list.rs y grid.rs. El usuario está moviendo panes, no navegando — los rects no se necesitan en ese momento.

### Paso 4 — Migrar split shares a `Transition::new()` (medio)

Reemplazar el loop manual de `dock_split_shares()` con el patrón `Transition` que ya usa la sidebar. Esto elimina el `request_animation_frame()` manual y delega al sistema de GPUI que es view-scoped.

```rust
// Patrón de sidebar/mod.rs:540
Transition::new(Duration::from_millis(90))
    .flex_grow(from_share, to_share)
    .apply(element, animation_id)
```

### Paso 5 — Entity<DockAnimator> separado de App (arquitectural)

Mover todo el estado de `dock_split_animations` a un `Entity<DockAnimator>` dedicado. El `App` lo observa pero no es invalidado por él. Solo `DockAnimator` se re-renderiza cada frame de animación.

Este es el patrón correcto de GPUI (como `PaletteHost`, `goto_overlay`) y eliminaría el problema de raíz independientemente de otras optimizaciones.

---

## Resumen de Impactos

| Fix | Archivo | Dificultad | Impacto |
|-----|---------|------------|---------|
| Borrar `cx.notify()` en `render.rs:275` | `dock/render.rs` | **1 línea** | Crítico |
| `DURATION` 260ms → 90ms | `app.rs` / `motion.rs` | **1 línea** | Alto |
| Skipear rect_recorder durante split anims | `list.rs`, `grid.rs` | Pequeño | Medio |
| `Transition::new()` para split shares | `dock/render.rs` | Medio | Alto |
| `Entity<DockAnimator>` | Múltiples archivos | Grande | Definitivo |

---

## Archivos Clave

- `atlas-ui/src/ui/dock/render.rs` — líneas 65, 87, 178, 249, 274-275
- `atlas-ui/src/views/list.rs:104` — rect_recorder on_prepaint
- `atlas-ui/src/views/grid.rs:157` — rect_recorder on_prepaint
- `atlas-ui/src/ui/root.rs:204` — comentario del fix parcial previo
- `atlas-ui/src/app.rs:429` — DURATION constant
- `gpui-component-fork/src/sidebar/mod.rs:540` — patrón Transition correcto
