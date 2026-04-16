# Fix Plan 02 — Security Issues

## Priority: HIGH

---

### 2.1 — No attribute value escaping (svg_edit.rs:75-122)

**Problem:**
`set_attribute()` inserts values directly into SVG without escaping XML special characters. An ID like `test" onclick="alert(1)` produces attribute injection.

**Fix:**
Add an XML escape helper and use it in all attribute insertions:
```rust
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('"', "&quot;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('\'', "&apos;")
}
```
Apply in `set_attribute()`:
```rust
let insertion = format!(" id=\"{}\"", escape_xml_attr(id));
```
And in every `format!(...\"{}\"..)` pattern that inserts user-supplied values.

---

### 2.2 — String matching ignores XML context (svg_edit.rs, multiple locations)

**Problem:**
Pattern matches like `.contains("id=\"")` match inside comments, CDATA sections, and string literals. A crafted SVG with `<!-- id="fake" -->` could cause edits to target the wrong element.

**Affected locations:**
- `auto_assign_ids()` — tag detection
- `set_attribute()` — attribute finding
- `delete_element()` — element finding
- `set_visibility()` — style attribute checks

**Fix (short-term):**
Add a helper to check if a position is inside a comment or CDATA:
```rust
fn is_inside_comment(svg: &str, pos: usize) -> bool {
    let before = &svg[..pos];
    let last_comment_open = before.rfind("<!--");
    let last_comment_close = before.rfind("-->");
    match (last_comment_open, last_comment_close) {
        (Some(o), Some(c)) => o > c,
        (Some(_), None) => true,
        _ => false,
    }
}
```
Use this check before acting on any pattern match.

**Fix (long-term):**
Consider migrating element-finding logic to use roxmltree (already a dependency) for locating elements, then only use string manipulation for the actual edit. This would make all XML context handling correct by default.
