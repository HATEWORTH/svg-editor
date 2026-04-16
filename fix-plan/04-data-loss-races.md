# Fix Plan 04 — Data Loss & Race Conditions

## Priority: MEDIUM

---

### 4.1 — Debounce resets indefinitely (watcher.rs:49-66)

**Problem:**
Debounce timer resets on every new event. Rapid successive writes (e.g., Claude writing multiple SVG edits) delay reload indefinitely — the user never sees updates until writes stop for 200ms.

**Fix:**
Track both first event time and last event time. Emit after 200ms of quiet OR 1 second max wait:
```rust
pub fn poll(&mut self) -> Option<PathBuf> {
    while let Ok(path) = self.rx.try_recv() {
        if self.pending.is_none() {
            self.first_event = Instant::now();
        }
        self.pending = Some(path);
        self.last_event = Instant::now();
    }

    if let Some(path) = &self.pending {
        let quiet_enough = self.last_event.elapsed() >= Duration::from_millis(200);
        let waited_too_long = self.first_event.elapsed() >= Duration::from_secs(1);
        if quiet_enough || waited_too_long {
            let p = path.clone();
            self.pending = None;
            return Some(p);
        }
    }
    None
}
```
Add `first_event: Instant` field to the watcher struct.

---

### 4.2 — External reload discards in-progress edits

**Problem:**
If Claude writes to the SVG while the user is mid-drag (visual preview), the file watcher reload replaces `svg_content`, dropping the user's uncommitted changes without warning.

**Fix (option A — simple):**
Skip reload while a drag is in progress:
```rust
// In the reload handler (app.rs)
if self.canvas.is_dragging() {
    self.pending_reload = Some(path);
    return; // defer until drag ends
}
```
Then in drag-end / commit:
```rust
if let Some(path) = self.pending_reload.take() {
    self.reload_from(path);
}
```

**Fix (option B — robust):**
Show a notification: "File changed externally. Reload? [Yes] [Ignore]" — but this adds UI complexity.

---

### 4.3 — Whitespace stripping after delete (svg_edit.rs:249,254)

**Problem:**
After deleting an element, trailing whitespace is stripped. If the SVG has unusual structure where meaningful content follows only whitespace, it could be consumed.

**Fix:**
Limit whitespace stripping to a single newline + indentation:
```rust
// Only strip one trailing newline and its leading whitespace
let end = if svg[end..].starts_with('\n') {
    end + 1 + svg[end + 1..].len() - svg[end + 1..].trim_start_matches(|c: char| c == ' ' || c == '\t').len()
} else {
    end
};
```
Or more conservatively, just strip `\n` but not arbitrary whitespace.
