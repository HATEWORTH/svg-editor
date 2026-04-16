# Fix Plan 03 — Logic Errors

## Priority: MEDIUM

---

### 3.1 — Animation freeze logic is dead code (animation.rs:110-113)

**Problem:**
```rust
if local_t > total {
    if anim.fill == "freeze" { anim.dur }  // evaluates but never assigned
    else { return None; }
}
let cycle_t = local_t % anim.dur;  // always executes
```
The `anim.dur` expression on the freeze branch is never assigned to `cycle_t`. Freeze behavior (holding last frame) doesn't work.

**Fix:**
```rust
let cycle_t = if local_t > total {
    if anim.fill == "freeze" {
        anim.dur  // clamp to end of last cycle
    } else {
        return None;
    }
} else {
    local_t % anim.dur
};
```

---

### 3.2 — Duplicate layer names (svg_ops.rs:60-83)

**Problem:**
All unnamed rects get name "Rect", all circles get "Circle", etc. Indistinguishable in layer panel.

**Fix:**
Include a counter or element index:
```rust
match tag.as_str() {
    "rect" => format!("Rect {}", child_index),
    "circle" => format!("Circle {}", child_index),
    // ...
}
```
Or use the element's ID if available: `format!("Rect ({})", id)`.

---

### 3.3 — Fragile toggle_expanded fallback (layers.rs:262)

**Problem:**
When ID is empty, falls back to matching by name+depth. Two elements with the same name at the same depth both toggle.

**Fix:**
Since `auto_assign_ids()` ensures every element has an ID, remove the name+depth fallback or log a warning when it's used:
```rust
fn toggle_expanded(nodes: &mut [LayerNode], id: &str, _name: &str, _depth: usize) {
    for n in nodes.iter_mut() {
        if !id.is_empty() && n.id == id {
            n.expanded = !n.expanded;
            return;
        }
    }
}
```

---

### 3.4 — No loop guard in auto_assign_ids() (svg_edit.rs:41)

**Problem:**
Malformed SVG with unclosed tags can cause the search loop to never advance, looping forever.

**Fix:**
Add an iteration limit:
```rust
let max_iterations = svg.len(); // can't have more tags than bytes
let mut iterations = 0;
loop {
    iterations += 1;
    if iterations > max_iterations { break; }
    // ... existing logic
}
```

---

### 3.5 — Attribute parsing breaks on escaped quotes (svg_edit.rs:99-106)

**Problem:**
`find('"')` to locate attribute value end fails on values containing escaped quotes like `fill="color\"name"`.

**Fix:**
Skip escaped quotes when searching for the closing quote:
```rust
fn find_unescaped_quote(s: &str) -> Option<usize> {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
            return Some(i);
        }
        i += 1;
    }
    None
}
```
Note: In valid XML, `"` inside attributes uses `&quot;` not `\"`, but defensive handling is still wise since users may produce non-standard SVGs.
