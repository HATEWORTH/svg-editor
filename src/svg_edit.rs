use crate::layers::ReorderDir;

/// Auto-assign IDs to SVG elements that don't have one.
/// This makes every element individually selectable and movable.
pub fn auto_assign_ids(svg: &str) -> String {
    let tags = ["<rect ", "<rect\n", "<circle ", "<circle\n", "<ellipse ", "<ellipse\n",
                "<path ", "<path\n", "<text ", "<text\n", "<image ", "<image\n",
                "<line ", "<line\n", "<polygon ", "<polygon\n", "<polyline ", "<polyline\n",
                "<g ", "<g\n"];

    let mut result = svg.to_string();
    let mut counter = 0u32;
    let mut search_from = 0;

    loop {
        // Find the next element tag that might need an ID
        let mut earliest: Option<(usize, &str)> = None;
        for tag in &tags {
            if let Some(pos) = result[search_from..].find(tag) {
                let abs = search_from + pos;
                if earliest.is_none() || abs < earliest.unwrap().0 {
                    earliest = Some((abs, tag));
                }
            }
        }

        let (pos, tag) = match earliest {
            Some(x) => x,
            None => break,
        };

        // Find end of this opening tag
        let tag_end = match result[pos..].find('>') {
            Some(e) => pos + e,
            None => break,
        };

        let opening = &result[pos..=tag_end];

        // Skip if already has an id
        if opening.contains("id=\"") {
            search_from = tag_end + 1;
            continue;
        }

        // Skip defs children (stop, linearGradient, etc.)
        // Check if we're inside a <defs> block
        let before = &result[..pos];
        let defs_open = before.rfind("<defs");
        let defs_close = before.rfind("</defs>");
        let in_defs = match (defs_open, defs_close) {
            (Some(o), Some(c)) => o > c,
            (Some(_), None) => true,
            _ => false,
        };
        if in_defs {
            search_from = tag_end + 1;
            continue;
        }

        // Generate a descriptive ID from the tag and key attributes
        let tag_name = tag.trim_start_matches('<').trim();
        let opening_str = result[pos..=tag_end].to_string();
        let desc = generate_element_id(tag_name, &opening_str, counter);
        counter += 1;

        // Insert id after the tag name
        let insert_pos = pos + tag.trim_end().len();
        let insertion = format!(" id=\"{}\"", desc);
        result.insert_str(insert_pos, &insertion);
        search_from = insert_pos + insertion.len();
    }

    result
}

/// Generate a descriptive element ID from tag name and attributes.
/// Produces IDs like "red-rect-0", "blue-circle-1", "text-hello-2".
fn generate_element_id(tag_name: &str, opening_tag: &str, counter: u32) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Extract fill color for a prefix
    let color = extract_attr_value(opening_tag, "fill");
    if let Some(ref c) = color {
        if c != "none" {
            let name = quick_color_label(c);
            if !name.is_empty() {
                parts.push(name);
            }
        }
    }

    // Tag name
    parts.push(tag_name.to_string());

    // For text elements, try to grab text content (not in opening tag, so skip)
    // The layer panel will handle display names via describe_element

    let base = if parts.is_empty() {
        format!("el-{}", counter)
    } else {
        format!("{}-{}", parts.join("-"), counter)
    };

    // Sanitize: IDs can't have spaces, must be valid XML
    base.replace(' ', "-").to_lowercase()
}

/// Quick color label from a color string (for ID generation).
fn quick_color_label(color: &str) -> String {
    let c = color.trim().to_lowercase();
    // Named colors
    for name in &["red", "blue", "green", "yellow", "orange", "purple", "pink",
                   "cyan", "white", "black", "gray", "brown", "navy", "teal", "gold"] {
        if c == *name { return name.to_string(); }
    }
    // Hex colors — approximate
    if c.starts_with('#') && c.len() >= 7 {
        let r = u8::from_str_radix(&c[1..3], 16).unwrap_or(128);
        let g = u8::from_str_radix(&c[3..5], 16).unwrap_or(128);
        let b = u8::from_str_radix(&c[5..7], 16).unwrap_or(128);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        if max < 30 { return "black".into(); }
        if min > 225 { return "white".into(); }
        if max - min < 25 { return "gray".into(); }
        if r > g + 50 && r > b + 50 { return "red".into(); }
        if g > r + 50 && g > b + 50 { return "green".into(); }
        if b > r + 50 && b > g + 50 { return "blue".into(); }
        if r > 200 && g > 200 && b < 100 { return "yellow".into(); }
        if r > 200 && g > 100 && b < 80 { return "orange".into(); }
    }
    String::new()
}

/// Extract an attribute value from a raw opening tag string.
fn extract_attr_value(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let start = tag.find(&pattern)?;
    let val_start = start + pattern.len();
    let val_end = tag[val_start..].find('"')?;
    Some(tag[val_start..val_start + val_end].to_string())
}

/// Set or update an attribute on an element found by ID.
/// Returns the modified SVG string.
pub fn set_attribute(svg: &str, element_id: &str, attr_name: &str, attr_value: &str) -> String {
    // Find the element's opening tag by its id
    let id_pattern = format!("id=\"{}\"", element_id);
    let id_pos = match svg.find(&id_pattern) {
        Some(p) => p,
        None => return svg.to_string(),
    };

    // Walk backwards to find the '<' that starts this tag
    let tag_start = match svg[..id_pos].rfind('<') {
        Some(p) => p,
        None => return svg.to_string(),
    };

    // Walk forward to find the '>' or '/>' that ends the opening tag
    let tag_end = match svg[tag_start..].find('>') {
        Some(p) => tag_start + p,
        None => return svg.to_string(),
    };

    let opening_tag = &svg[tag_start..=tag_end];

    // Check if attribute already exists in this tag (with whitespace boundary check)
    let attr_pattern = format!("{}=\"", attr_name);
    let attr_start_rel = {
        let mut found = None;
        let mut sf = 0;
        while let Some(rel) = opening_tag[sf..].find(&attr_pattern) {
            let abs = sf + rel;
            if abs == 0 || opening_tag.as_bytes()[abs - 1].is_ascii_whitespace() {
                found = Some(abs);
                break;
            }
            sf = abs + 1;
        }
        found
    };
    if let Some(attr_start_rel) = attr_start_rel {
        // Replace existing attribute value
        let attr_start = tag_start + attr_start_rel + attr_pattern.len();
        let attr_end = match svg[attr_start..].find('"') {
            Some(p) => attr_start + p,
            None => return svg.to_string(),
        };
        format!("{}{}{}", &svg[..attr_start], attr_value, &svg[attr_end..])
    } else {
        // Insert new attribute before the closing '>' or '/>'
        let insert_pos = if svg[..=tag_end].ends_with("/>") {
            tag_end - 1
        } else {
            tag_end
        };
        format!(
            "{} {}=\"{}\"{}",
            &svg[..insert_pos],
            attr_name,
            attr_value,
            &svg[insert_pos..]
        )
    }
}

/// Get the current transform translate values for an element.
pub fn get_translate(svg: &str, element_id: &str) -> (f32, f32) {
    let transform = get_attribute(svg, element_id, "transform").unwrap_or_default();
    parse_translate(&transform)
}

/// Get an attribute value from an element by ID.
pub fn get_attribute(svg: &str, element_id: &str, attr_name: &str) -> Option<String> {
    let id_pattern = format!("id=\"{}\"", element_id);
    let id_pos = svg.find(&id_pattern)?;
    let tag_start = svg[..id_pos].rfind('<')?;
    let tag_end = tag_start + svg[tag_start..].find('>')?;
    let tag = &svg[tag_start..=tag_end];

    // Search for the attribute with a whitespace boundary so e.g. "d=" doesn't match inside "id="
    let attr_pattern = format!("{}=\"", attr_name);
    let mut search_from = 0;
    loop {
        let rel = tag[search_from..].find(&attr_pattern)?;
        let abs = search_from + rel;
        // Valid match: at start of tag attributes, or preceded by whitespace
        if abs == 0 || tag.as_bytes()[abs - 1].is_ascii_whitespace() {
            let val_start = abs + attr_pattern.len();
            let val_end = val_start + tag[val_start..].find('"')?;
            return Some(tag[val_start..val_end].to_string());
        }
        // Not a boundary match — keep searching past this occurrence
        search_from = abs + 1;
    }
}

/// Parse translate(x, y) from a transform string.
fn parse_translate(transform: &str) -> (f32, f32) {
    if let Some(start) = transform.find("translate(") {
        let inner_start = start + "translate(".len();
        if let Some(end) = transform[inner_start..].find(')') {
            let inner = &transform[inner_start..inner_start + end];
            let parts: Vec<f32> = inner
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            return (
                parts.first().copied().unwrap_or(0.0),
                parts.get(1).copied().unwrap_or(0.0),
            );
        }
    }
    (0.0, 0.0)
}

/// Set translate on an element, preserving other transforms.
pub fn set_translate(svg: &str, element_id: &str, tx: f32, ty: f32) -> String {
    let current = get_attribute(svg, element_id, "transform").unwrap_or_default();
    let new_translate = format!("translate({:.1} {:.1})", tx, ty);

    let new_transform = if current.contains("translate(") {
        // Replace existing translate
        let re_start = current.find("translate(").unwrap();
        let re_end = re_start + current[re_start..].find(')').unwrap() + 1;
        format!("{}{}{}", &current[..re_start], new_translate, &current[re_end..])
    } else if current.is_empty() {
        new_translate
    } else {
        format!("{} {}", new_translate, current)
    };

    set_attribute(svg, element_id, "transform", &new_transform)
}

/// Set visibility (display) on an element.
pub fn set_visibility(svg: &str, element_id: &str, visible: bool) -> String {
    if visible {
        // Remove display:none if present
        let style = get_attribute(svg, element_id, "style").unwrap_or_default();
        if style.contains("display:none") || style.contains("display: none") {
            let new_style = style
                .replace("display:none", "")
                .replace("display: none", "")
                .replace(";;", ";")
                .trim_matches(';')
                .to_string();
            if new_style.is_empty() {
                // Remove the style attribute entirely - just set it empty for now
                set_attribute(svg, element_id, "style", "")
            } else {
                set_attribute(svg, element_id, "style", &new_style)
            }
        } else {
            svg.to_string()
        }
    } else {
        let style = get_attribute(svg, element_id, "style").unwrap_or_default();
        if style.is_empty() {
            set_attribute(svg, element_id, "style", "display:none")
        } else if !style.contains("display:none") {
            set_attribute(svg, element_id, "style", &format!("{};display:none", style))
        } else {
            svg.to_string()
        }
    }
}

/// Set the forge:visible attribute.
pub fn set_forge_visible(svg: &str, element_id: &str, visible: bool) -> String {
    let svg = set_attribute(svg, element_id, "forge:visible", if visible { "true" } else { "false" });
    set_visibility(&svg, element_id, visible)
}

/// Delete an element by ID (removes from opening tag to closing tag).
pub fn delete_element(svg: &str, element_id: &str) -> String {
    let id_pattern = format!("id=\"{}\"", element_id);
    let id_pos = match svg.find(&id_pattern) {
        Some(p) => p,
        None => return svg.to_string(),
    };

    let tag_start = match svg[..id_pos].rfind('<') {
        Some(p) => p,
        None => return svg.to_string(),
    };

    // Get the tag name
    let tag_name_end = svg[tag_start + 1..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(0)
        + tag_start
        + 1;
    let tag_name = &svg[tag_start + 1..tag_name_end];

    // Find closing tag
    let closing = format!("</{}>", tag_name);
    if let Some(close_pos) = svg[tag_start..].find(&closing) {
        let end = tag_start + close_pos + closing.len();
        // Also eat trailing whitespace/newline
        let end = svg[end..].find(|c: char| !c.is_whitespace()).map_or(end, |p| end + p);
        format!("{}{}", &svg[..tag_start], &svg[end..])
    } else {
        // Self-closing tag
        let tag_end = svg[tag_start..].find("/>").unwrap_or(0) + tag_start + 2;
        let tag_end = svg[tag_end..].find(|c: char| !c.is_whitespace()).map_or(tag_end, |p| tag_end + p);
        format!("{}{}", &svg[..tag_start], &svg[tag_end..])
    }
}

/// Extract a full element string by ID (opening tag through closing tag).
fn extract_element(svg: &str, element_id: &str) -> Option<(usize, usize, String)> {
    let id_pattern = format!("id=\"{}\"", element_id);
    let id_pos = svg.find(&id_pattern)?;
    let tag_start = svg[..id_pos].rfind('<')?;

    // Get tag name
    let tag_name_end = svg[tag_start + 1..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')? + tag_start + 1;
    let tag_name = &svg[tag_start + 1..tag_name_end];

    let closing = format!("</{}>", tag_name);
    if let Some(close_rel) = svg[tag_start..].find(&closing) {
        let end = tag_start + close_rel + closing.len();
        Some((tag_start, end, svg[tag_start..end].to_string()))
    } else {
        // Self-closing
        let end = svg[tag_start..].find("/>")? + tag_start + 2;
        Some((tag_start, end, svg[tag_start..end].to_string()))
    }
}

/// Reorder an element relative to a target element.
pub fn reorder_element(svg: &str, drag_id: &str, target_id: &str, dir: ReorderDir) -> String {
    if drag_id == target_id { return svg.to_string(); }

    // Extract the dragged element
    let (drag_start, drag_end, drag_content) = match extract_element(svg, drag_id) {
        Some(x) => x,
        None => return svg.to_string(),
    };

    // Remove dragged element first
    let svg_without = format!("{}{}", &svg[..drag_start], &svg[drag_end..]);

    // Find target in the modified string
    let target_pattern = format!("id=\"{}\"", target_id);
    let target_id_pos = match svg_without.find(&target_pattern) {
        Some(p) => p,
        None => return svg.to_string(),
    };
    let target_start = match svg_without[..target_id_pos].rfind('<') {
        Some(p) => p,
        None => return svg.to_string(),
    };

    match dir {
        ReorderDir::Before => {
            format!("{}{}\n  {}", &svg_without[..target_start], drag_content, &svg_without[target_start..])
        }
        ReorderDir::After => {
            // Find end of target element
            let tag_name_end = svg_without[target_start + 1..]
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(0) + target_start + 1;
            let tag_name = &svg_without[target_start + 1..tag_name_end];
            let closing = format!("</{}>", tag_name);

            let target_end = if let Some(p) = svg_without[target_start..].find(&closing) {
                target_start + p + closing.len()
            } else if let Some(p) = svg_without[target_start..].find("/>") {
                target_start + p + 2
            } else {
                return svg.to_string();
            };

            format!("{}\n  {}{}", &svg_without[..target_end], drag_content, &svg_without[target_end..])
        }
        ReorderDir::Into => {
            // Insert as last child of target group
            let closing = format!("</g>");
            // Find the closing tag of the target group
            if let Some(close_rel) = svg_without[target_start..].find(&closing) {
                let insert_pos = target_start + close_rel;
                format!("{}  {}\n  {}", &svg_without[..insert_pos], drag_content, &svg_without[insert_pos..])
            } else {
                svg.to_string()
            }
        }
    }
}

// ─── Transform matrix operations ─────────────────────

/// 2D affine transform as [a, b, c, d, e, f] where:
///   x' = a*x + c*y + e
///   y' = b*x + d*y + f
pub type Matrix = [f32; 6];

pub fn identity() -> Matrix {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

pub fn compose(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

pub fn transform_point(m: &Matrix, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

pub fn inverse(m: &Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-10 {
        return None;
    }
    let id = 1.0 / det;
    Some([
        m[3] * id,
        -m[1] * id,
        -m[2] * id,
        m[0] * id,
        (m[2] * m[5] - m[3] * m[4]) * id,
        (m[1] * m[4] - m[0] * m[5]) * id,
    ])
}

pub fn scale_around(sx: f32, sy: f32, ax: f32, ay: f32) -> Matrix {
    [sx, 0.0, 0.0, sy, ax * (1.0 - sx), ay * (1.0 - sy)]
}

pub fn rotate_around(angle_rad: f32, cx: f32, cy: f32) -> Matrix {
    let cos = angle_rad.cos();
    let sin = angle_rad.sin();
    [
        cos,
        sin,
        -sin,
        cos,
        cx * (1.0 - cos) + cy * sin,
        cy * (1.0 - cos) - cx * sin,
    ]
}

/// Parse an SVG `transform` attribute into an affine matrix.
pub fn parse_transform(s: &str) -> Matrix {
    let s = s.trim();
    if s.is_empty() {
        return identity();
    }

    let mut result = identity();
    let bytes = s.as_bytes();
    let mut pos = 0;

    while pos < s.len() {
        while pos < s.len() && (bytes[pos] == b' ' || bytes[pos] == b',') {
            pos += 1;
        }
        if pos >= s.len() {
            break;
        }

        let name_start = pos;
        while pos < s.len() && bytes[pos].is_ascii_alphabetic() {
            pos += 1;
        }
        let name = &s[name_start..pos];

        while pos < s.len() && bytes[pos] != b'(' {
            pos += 1;
        }
        if pos >= s.len() {
            break;
        }
        pos += 1;

        let params_start = pos;
        while pos < s.len() && bytes[pos] != b')' {
            pos += 1;
        }
        let params_str = &s[params_start..pos];
        if pos < s.len() {
            pos += 1;
        }

        let p: Vec<f32> = params_str
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        let m = match name {
            "translate" => {
                let tx = p.first().copied().unwrap_or(0.0);
                let ty = p.get(1).copied().unwrap_or(0.0);
                [1.0, 0.0, 0.0, 1.0, tx, ty]
            }
            "scale" => {
                let sx = p.first().copied().unwrap_or(1.0);
                let sy = p.get(1).copied().unwrap_or(sx);
                [sx, 0.0, 0.0, sy, 0.0, 0.0]
            }
            "rotate" => {
                let angle = p.first().copied().unwrap_or(0.0).to_radians();
                let cx = p.get(1).copied().unwrap_or(0.0);
                let cy = p.get(2).copied().unwrap_or(0.0);
                rotate_around(angle, cx, cy)
            }
            "matrix" if p.len() >= 6 => [p[0], p[1], p[2], p[3], p[4], p[5]],
            "skewX" => {
                let a = p.first().copied().unwrap_or(0.0).to_radians();
                [1.0, 0.0, a.tan(), 1.0, 0.0, 0.0]
            }
            "skewY" => {
                let a = p.first().copied().unwrap_or(0.0).to_radians();
                [1.0, a.tan(), 0.0, 1.0, 0.0, 0.0]
            }
            _ => identity(),
        };

        result = compose(&result, &m);
    }

    result
}

/// Convert a matrix to an SVG transform attribute string.
pub fn matrix_to_string(m: &Matrix) -> String {
    let is_ident = (m[0] - 1.0).abs() < 1e-4
        && m[1].abs() < 1e-4
        && m[2].abs() < 1e-4
        && (m[3] - 1.0).abs() < 1e-4
        && m[4].abs() < 0.05
        && m[5].abs() < 0.05;
    if is_ident {
        return String::new();
    }

    let is_translate = (m[0] - 1.0).abs() < 1e-4
        && m[1].abs() < 1e-4
        && m[2].abs() < 1e-4
        && (m[3] - 1.0).abs() < 1e-4;
    if is_translate {
        return format!("translate({:.1} {:.1})", m[4], m[5]);
    }

    format!(
        "matrix({:.6} {:.6} {:.6} {:.6} {:.2} {:.2})",
        m[0], m[1], m[2], m[3], m[4], m[5]
    )
}

/// Get the tag name of an element by its ID.
pub fn get_element_tag(svg: &str, element_id: &str) -> Option<String> {
    let id_pattern = format!("id=\"{}\"", element_id);
    let id_pos = svg.find(&id_pattern)?;
    let tag_start = svg[..id_pos].rfind('<')?;
    let name_end = svg[tag_start + 1..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .map(|p| p + tag_start + 1)?;
    Some(svg[tag_start + 1..name_end].to_string())
}

/// Set the full transform attribute on an element.
pub fn set_transform(svg: &str, element_id: &str, transform_str: &str) -> String {
    if transform_str.is_empty() {
        let current = get_attribute(svg, element_id, "transform");
        if current.is_some() {
            set_attribute(svg, element_id, "transform", "")
        } else {
            svg.to_string()
        }
    } else {
        set_attribute(svg, element_id, "transform", transform_str)
    }
}
