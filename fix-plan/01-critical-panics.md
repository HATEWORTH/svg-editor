# Fix Plan 01 — Critical Panics & Crashes

## Priority: CRITICAL

These are the most likely sources of crashes from malformed SVG input.

---

### 1.1 — `set_translate()` unwraps (svg_edit.rs:172-173)

**Problem:**
```rust
let re_start = current.find("translate(").unwrap();
let re_end = re_start + current[re_start..].find(')').unwrap() + 1;
```
If transform contains `translate(` but no closing `)`, the second `unwrap()` panics.

**Fix:**
Replace with `if let` or `?` operator:
```rust
let re_start = match current.find("translate(") {
    Some(pos) => pos,
    None => { /* handle: append new translate */ }
};
let re_end = match current[re_start..].find(')') {
    Some(pos) => re_start + pos + 1,
    None => return svg.to_string(), // malformed, return unchanged
};
```

---

### 1.2 — `delete_element()` tag name extraction (svg_edit.rs:237-241)

**Problem:**
```rust
let tag_name_end = svg[tag_start + 1..]
    .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
    .unwrap_or(0)
    + tag_start + 1;
```
`unwrap_or(0)` on failed find produces `tag_name_end = tag_start + 1`, yielding an empty tag name. `format!("</{}>", tag_name)` becomes `</>`.

**Fix:**
Return early if tag name can't be extracted:
```rust
let tag_name_end = match svg[tag_start + 1..].find(|c: char| c.is_whitespace() || c == '>' || c == '/') {
    Some(0) | None => return svg.to_string(), // malformed tag
    Some(p) => p + tag_start + 1,
};
```

---

### 1.3 — Self-closing tag detection (svg_edit.rs:253)

**Problem:**
```rust
let tag_end = svg[tag_start..].find("/>").unwrap_or(0) + tag_start + 2;
```
If `"/>` not found, `tag_end` = `tag_start + 2`, which is wrong and can truncate content.

**Fix:**
```rust
let tag_end = match svg[tag_start..].find("/>") {
    Some(p) => p + tag_start + 2,
    None => return svg.to_string(), // unclosed self-closing tag
};
```

---

### 1.4 — Division by zero in animation (canvas.rs:208)

**Problem:**
```rust
self.anim_time = self.anim_time % self.anim_duration;
```
When `anim_duration` is 0.0, this produces NaN, permanently breaking playback.

**Fix:**
```rust
if self.anim_duration > 0.0 {
    self.anim_time = self.anim_time % self.anim_duration;
} else {
    self.anim_time = 0.0;
}
```

---

### 1.5 — `expect()` calls in main.rs:30-36

**Problem:**
Two `expect()` calls panic if directory creation or canonicalize fails.

**Fix:**
Use proper error reporting and exit gracefully:
```rust
let project_dir = match PathBuf::from(&cli.project_dir).canonicalize() {
    Ok(p) => p,
    Err(_) => {
        let p = PathBuf::from(&cli.project_dir);
        if let Err(e) = std::fs::create_dir_all(&p) {
            eprintln!("Failed to create project directory: {}", e);
            std::process::exit(1);
        }
        match p.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to resolve project directory: {}", e);
                std::process::exit(1);
            }
        }
    }
};
```
