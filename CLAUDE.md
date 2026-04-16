# SVG Forge — CLAUDE.md

## What is this project?
A native desktop collaborative SVG editor built in Rust. Claude and a human can both edit the same SVG file — Claude via code (writing to disk), the user via the visual editor — with the filesystem as the sync layer. The file watcher detects external changes and live-reloads.

## Build & Run
```bash
cargo build --release
target/release/svg-forge.exe [PROJECT_DIR]
# or simply:
run.bat [PROJECT_DIR]
```
No web server, no browser. Pure native GUI via egui + GPU rendering.

## Architecture

### Tech Stack
- **GUI:** egui/eframe (native GPU-accelerated window)
- **SVG Rendering:** resvg + usvg + tiny-skia (renders SVG to pixel buffer, displayed as texture)
- **SVG Parsing:** roxmltree (read-only XML parsing for layer tree, attribute extraction)
- **SVG Editing:** String manipulation in `svg_edit.rs` (find element by ID, modify attributes)
- **File Watching:** notify crate with 200ms debounce
- **File Dialogs:** rfd (native OS file picker)
- **Animation:** Custom SMIL evaluator in `animation.rs` (parses <animate>/<animateTransform>, interpolates at time t, bakes values into temp SVG for rendering)

### Source Files (src/)
| File | Purpose |
|------|---------|
| `main.rs` | CLI args (clap), eframe window setup, dark theme |
| `app.rs` | Main app struct, menu bar (File/Edit/Window), keyboard shortcuts, drag-drop, recent files, layer action wiring |
| `canvas.rs` | SVG texture rendering, zoom/pan, selection (bounding boxes from usvg), move tool (drag with visual preview, commit on release), resize/rotate handles (visual), undo/redo stack, animation playback tick |
| `layers.rs` | Tree layer panel with expand/collapse, visibility toggle, drag-to-reorder, move up/down buttons |
| `animation.rs` | SMIL parser (extracts animate/animateTransform/set elements), time evaluator (interpolates values at time t), bakes animated values into SVG string for rendering |
| `svg_edit.rs` | SVG string manipulation: set/get attributes by element ID, set_translate, set_visibility, delete_element, reorder_element, auto_assign_ids |
| `svg_ops.rs` | Layer tree parser (recursive roxmltree → LayerNode tree), mode detection |
| `project.rs` | File discovery, read/write SVG, new project creation |
| `watcher.rs` | File watcher with mpsc channel + 200ms debounce |

### Key Design Decisions

1. **SVG string as source of truth** — All edits modify the raw SVG string via `svg_edit.rs`. No mutable DOM. roxmltree is read-only (used for parsing), usvg is read-only (used for rendering + bounding boxes). Edits find elements by `id="..."` pattern in the string and do targeted attribute replacement.

2. **Auto-assigned IDs** — On load, `auto_assign_ids()` scans the SVG and gives every element without an `id` a unique one (`forge-0`, `forge-1`, ...). This makes every element individually selectable and movable. IDs inside `<defs>` are skipped.

3. **Transform-aware bounding boxes** — `collect_bboxes_ordered()` walks the usvg tree accumulating parent transforms. Bboxes are in global SVG coordinate space. Deduplication keeps one entry per unique ID. Hit testing walks in reverse z-order (topmost first).

4. **Move tool: visual preview → commit** — During drag, only `drag_offset` changes (no SVG modification). On release, `set_translate()` modifies the SVG and `commit()` pushes to undo stack + re-renders. This avoids expensive re-parsing on every frame.

5. **Animation via SMIL evaluation** — resvg strips SMIL tags during parsing. So `animation.rs` parses them from the raw SVG string, evaluates at current time `t`, and produces a modified SVG with baked attribute values. This temp SVG is what resvg renders. The original SVG with animation tags is preserved.

6. **forge: namespace** — Layers use `forge:name`, `forge:visible`, `forge:locked`, `forge:order` attributes on `<g>` elements. `forge:mode="animated"` on root `<svg>` indicates animation mode, but auto-detection also checks for SMIL tags.

### How Claude Should Edit SVGs
Write/modify the SVG file on disk. The file watcher picks up changes within ~200ms and live-reloads. Key conventions:
- Use `<g>` elements with `id` and `forge:name` attributes for layers
- Elements should have unique `id` attributes (auto-assigned on load if missing)
- Use `transform="translate(x y)"` for positioning
- For animations, use standard SMIL: `<animate>`, `<animateTransform>`, `<set>`
- The SVG is the single source of truth — what's on disk is what the user sees

### Known Limitations
- **Resize/rotate handles** are visual only — drag interaction not yet wired (move works)
- **SVG editing is string-based** — complex structural changes (wrapping in groups, splitting paths) need careful string manipulation
- **Animation** — SMIL is evaluated via custom interpolator, not a full SMIL engine. Supports `<animate>`, `<animateTransform>`, `<set>` with linear interpolation, color interpolation, multi-value keyframes. Does NOT support event-based triggers, `<animateMotion>` path following, or CSS animations
- **No drawing tools** — Can't create new shapes from the UI yet (import or Claude-generate them)
- **Text editing** — Can't edit text content from the UI

### Shortcuts
| Key | Action |
|-----|--------|
| Ctrl+N | New static project |
| Ctrl+O | Open file |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Delete | Delete selected element |
| Space | Play/pause animation (animated mode) |
| Scroll wheel | Zoom |
| Middle-click drag | Pan |
| Left click | Select element |
| Left drag | Move selected element |
