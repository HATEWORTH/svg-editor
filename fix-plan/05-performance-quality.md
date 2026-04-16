# Fix Plan 05 — Performance & Code Quality

## Priority: LOW

---

### 5.1 — rebuild_bboxes() reparses full SVG tree (canvas.rs:140-158)

**Problem:**
Called after every edit via `commit()` and on undo/redo. Parses full SVG with `usvg::Tree::from_str()` every time. Expensive on large SVGs — causes UI lag.

**Fix:**
Cache the usvg tree and only rebuild when SVG content actually changes:
```rust
fn rebuild_bboxes(&mut self) {
    // Skip if SVG hasn't changed since last rebuild
    let hash = hash_fast(&self.svg_content);
    if self.last_bbox_hash == Some(hash) { return; }
    self.last_bbox_hash = Some(hash);
    // ... existing logic
}
```
Or more simply, add a `bboxes_dirty` flag set on SVG changes, cleared after rebuild.

---

### 5.2 — Unbounded undo stack memory (canvas.rs:96)

**Problem:**
Stores up to 50 full SVG string clones. A 5MB SVG = 250MB for undo history.

**Fix (short-term):**
Add a total memory budget instead of just a count limit:
```rust
const MAX_UNDO_BYTES: usize = 50 * 1024 * 1024; // 50MB total

fn push_undo(&mut self) {
    self.undo_stack.push(self.svg_content.clone());
    // Trim by memory, not just count
    let mut total: usize = self.undo_stack.iter().map(|s| s.len()).sum();
    while total > MAX_UNDO_BYTES && self.undo_stack.len() > 1 {
        total -= self.undo_stack.remove(0).len();
    }
}
```

**Fix (long-term):**
Store diffs instead of full copies. Use a diffing library to compute and store only the changes between versions.

---

### 5.3 — Hand-rolled base64 encoder (app.rs:548)

**Problem:**
Custom base64 implementation instead of using the `base64` crate. Error-prone and unnecessary.

**Fix:**
Add `base64 = "0.22"` to Cargo.toml and replace:
```rust
use base64::{Engine, engine::general_purpose};

fn encode_image(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}
```

---

### 5.4 — Silent parse failures in animation (animation.rs:65, and others)

**Problem:**
Invalid values like `repeatCount="abc"` silently become 1.0, `dur="invalid"` becomes 0.0. No warnings to the user.

**Fix:**
Log warnings on parse failure:
```rust
let repeat_count = match s.parse::<f64>() {
    Ok(v) => RepeatCount::Definite(v),
    Err(_) => {
        eprintln!("Warning: invalid repeatCount value '{}', defaulting to 1", s);
        RepeatCount::Definite(1.0)
    }
};
```
Consider surfacing these in the status bar: `self.status_msg = format!("Warning: ...")`.

---

### 5.5 — Incomplete whitespace handling in tag detection (svg_edit.rs:6-9)

**Problem:**
Checks `<rect ` and `<rect\n` but misses `<rect\t` and `<rect\r`.

**Fix:**
Use a single check that handles any whitespace:
```rust
fn find_tag_start(svg: &str, from: usize, tag: &str) -> Option<usize> {
    let pattern = format!("<{}", tag);
    let mut pos = from;
    while let Some(found) = svg[pos..].find(&pattern) {
        let abs = pos + found;
        let after = abs + pattern.len();
        if after < svg.len() {
            let next_char = svg[after..].chars().next().unwrap();
            if next_char.is_whitespace() || next_char == '>' || next_char == '/' {
                return Some(abs);
            }
        }
        pos = abs + 1;
    }
    None
}
```
