# SVG Forge

A native desktop SVG editor designed for collaborative editing between humans and AI. Built in Rust with GPU-accelerated rendering.

![Static Mode](https://img.shields.io/badge/mode-static-green) ![Animated Mode](https://img.shields.io/badge/mode-animated-orange) ![Platform](https://img.shields.io/badge/platform-Windows-blue) ![Language](https://img.shields.io/badge/language-Rust-red)

## Overview

SVG Forge lets you visually edit SVG files while an AI assistant (like Claude) edits the same file via code. The filesystem is the sync layer — when the AI writes to the SVG file, the editor live-reloads within 200ms. When you make visual edits and save, the AI can read the updated file on its next prompt.

**No browser. No web server.** Pure native application using egui for the GUI and resvg for pixel-perfect SVG rendering.

## Features

### Editor
- **Visual canvas** with zoom (scroll wheel), pan (middle-click drag), and fit-to-view
- **Click to select** any SVG element — shows bounding box with resize handles and rotation handle
- **Drag to move** selected elements — smooth visual preview during drag, commits on release
- **Layer panel** with tree view — expand/collapse groups, toggle visibility, reorder layers
- **Undo/Redo** — full history stack (Ctrl+Z / Ctrl+Shift+Z)
- **Drag & drop** — drop SVG files to open, drop images (PNG/JPG/GIF/WebP/BMP) to embed as layers
- **Dark theme** with dot-grid background

### Animation
- **Auto-detection** — any SVG containing `<animate>`, `<animateTransform>`, or `<set>` elements automatically enters animated mode
- **Timeline** — play/pause button, time scrubber, current time display
- **SMIL playback** — evaluates animation attributes at current time with linear interpolation for numbers, colors, and transforms
- **Looping** — animations loop automatically
- **Space bar** — quick play/pause toggle

### File Management
- **File → New Static** — blank 1920x1080 canvas
- **File → New Animated** — blank animated canvas with timeline
- **File → Open** — native file picker (Ctrl+O)
- **File → Open Recent** — last 10 files, persisted across sessions
- **File → Save / Save As** — Ctrl+S / Ctrl+Shift+S
- **File → Import** — import SVG as layer into current project, or embed images
- **File → Export** — export as SVG or render to PNG at full resolution
- **Window → Layers Panel** — toggle visibility
- **Window → Fit to View / Reset Zoom**

### Live Collaboration with AI
- **File watcher** monitors the project directory for changes
- When an external editor (or Claude) modifies the SVG file, the canvas reloads automatically
- If you have unsaved visual edits, external changes are held back (no data loss)
- Claude can generate entire SVGs, add layers, modify animations — all picked up live

## Installation

### Prerequisites
- [Rust](https://rustup.rs/) (2021 edition or later)
- Windows 10/11 (tested on Windows 11 Pro)

### Build
```bash
git clone <repo>
cd svg-forge
cargo build --release
```

The binary is at `target/release/svg-forge.exe`.

### Run
```bash
# Open current directory as project
svg-forge

# Open a specific directory
svg-forge path/to/project

# Create a new project
svg-forge --new static my-project
svg-forge --new animated my-project

# Or use the batch file
run.bat [PROJECT_DIR]
```

## Usage

### Basic Workflow
1. Run `svg-forge` in your project directory
2. The editor opens with any existing `.svg` file, or creates a new one
3. Click elements to select them, drag to move
4. Use the layer panel to manage visibility and ordering
5. Ctrl+S to save

### Working with Claude
1. Start SVG Forge in your project directory
2. In Claude Code (or any AI assistant), ask it to create/modify the SVG file
3. Watch the canvas update live as Claude writes
4. Make visual adjustments in the editor, Ctrl+S to save
5. Tell Claude to read the file for the next iteration

### SVG Conventions
SVG Forge uses a `forge:` namespace for editor metadata:

```xml
<svg xmlns="http://www.w3.org/2000/svg"
     xmlns:forge="https://svgforge.dev/ns"
     viewBox="0 0 1920 1080"
     forge:mode="static"
     forge:version="1">
  <g id="layer-bg"
     forge:name="Background"
     forge:visible="true"
     forge:locked="false"
     forge:order="0">
    <rect width="1920" height="1080" fill="#1a1a2e"/>
  </g>
</svg>
```

- **Layers** are `<g>` elements with `forge:name`, `forge:visible`, `forge:locked`, `forge:order`
- **Mode** is set via `forge:mode="static"` or `forge:mode="animated"` on the root `<svg>`, or auto-detected from SMIL animation tags
- **IDs** are auto-assigned to elements without them on load (format: `forge-0`, `forge-1`, ...)

### Animation
Use standard SVG SMIL for animations:

```xml
<circle cx="100" cy="100" r="50" fill="red">
  <animate attributeName="cx" from="100" to="500" dur="2s" repeatCount="indefinite"/>
</circle>

<rect x="0" y="0" width="100" height="100" fill="blue">
  <animateTransform attributeName="transform" type="translate"
    from="0 0" to="300 200" dur="3s" repeatCount="indefinite"/>
</rect>
```

Supported animation elements:
- `<animate>` — attribute animation (numbers, colors)
- `<animateTransform>` — translate, rotate, scale, skew
- `<set>` — discrete attribute changes

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Ctrl+N** | New static project |
| **Ctrl+O** | Open file |
| **Ctrl+S** | Save |
| **Ctrl+Shift+S** | Save As |
| **Ctrl+Z** | Undo |
| **Ctrl+Shift+Z** | Redo |
| **Delete** | Delete selected element |
| **Space** | Play/pause animation |
| **Scroll wheel** | Zoom in/out |
| **Middle-click drag** | Pan canvas |
| **Left click** | Select element |
| **Left click + drag** | Move element |

## Project Structure

```
svg-forge/
├── Cargo.toml
├── run.bat
├── CLAUDE.md              # AI assistant context
├── README.md
├── src/
│   ├── main.rs            # CLI args, window setup
│   ├── app.rs             # Menu bar, shortcuts, file ops, main loop
│   ├── canvas.rs          # SVG rendering, zoom/pan, selection, move tool
│   ├── layers.rs          # Layer tree panel, reorder, visibility
│   ├── animation.rs       # SMIL parser + evaluator
│   ├── svg_edit.rs        # SVG string manipulation (set attrs, translate, delete, reorder)
│   ├── svg_ops.rs         # Layer tree parser, mode detection
│   ├── project.rs         # File discovery, read/write
│   └── watcher.rs         # File watcher with debounce
└── examples/
    ├── static-project/
    │   └── drawing.svg
    └── animated-project/
        └── scene.svg
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| eframe + egui | Native GUI framework |
| resvg + usvg | SVG parsing and rendering |
| tiny-skia | 2D pixel rendering backend |
| roxmltree | Read-only XML/SVG parsing |
| notify | Cross-platform file watching |
| rfd | Native file dialogs |
| clap | CLI argument parsing |
| uuid | Unique ID generation |
| tracing | Logging |

## License

MIT
