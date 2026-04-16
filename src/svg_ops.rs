/// Validates SVG content by attempting to parse it.
pub fn validate_svg(content: &str) -> Result<(), String> {
    roxmltree::Document::parse(content).map_err(|e| format!("SVG parse error: {}", e))?;
    Ok(())
}

/// Detect project mode from SVG content.
/// Checks for forge:mode attribute, SMIL tags, and CSS animations.
pub fn detect_mode(content: &str) -> &'static str {
    if content.contains("forge:mode=\"animated\"") { return "animated"; }
    if crate::animation::has_animations(content) { return "animated"; }
    "static"
}

/// Recursive tree node for the layer panel.
#[derive(Debug, Clone)]
pub struct LayerNode {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub visible: bool,
    pub locked: bool,
    pub expanded: bool,
    pub children: Vec<LayerNode>,
    pub depth: usize,
}

/// Parse SVG into a tree of layer nodes (recursive).
pub fn parse_layer_tree(content: &str) -> Vec<LayerNode> {
    let doc = match roxmltree::Document::parse(content) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let root = doc.root_element();
    let mut nodes = Vec::new();
    for child in root.children() {
        if child.is_element() {
            if let Some(node) = parse_node(&child, 0) {
                nodes.push(node);
            }
        }
    }
    nodes
}

fn parse_node(node: &roxmltree::Node, depth: usize) -> Option<LayerNode> {
    if !node.is_element() { return None; }

    let tag = node.tag_name().name().to_string();
    // Skip defs, style, metadata
    if tag == "defs" || tag == "style" || tag == "metadata" { return None; }

    let id = node.attribute("id").unwrap_or("").to_string();
    let forge_ns = "https://svgforge.dev/ns";

    let name = node.attribute((forge_ns, "name"))
        .map(|s| s.to_string())
        .or_else(|| {
            // Skip generic forge IDs — generate a better name instead
            if !id.is_empty() && !id.starts_with("forge-") {
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| describe_element(node, &tag));

    let visible = node.attribute((forge_ns, "visible")).unwrap_or("true") == "true"
        && !node.attribute("style").unwrap_or("").contains("display:none");
    let locked = node.attribute((forge_ns, "locked")).unwrap_or("false") == "true";

    let mut children = Vec::new();
    for child in node.children() {
        if child.is_element() {
            if let Some(child_node) = parse_node(&child, depth + 1) {
                children.push(child_node);
            }
        }
    }

    Some(LayerNode {
        id,
        name,
        tag,
        visible,
        locked,
        expanded: depth == 0, // Top-level groups start expanded
        children,
        depth,
    })
}

/// Generate a descriptive name for an element based on its tag, color, text, and size.
fn describe_element(node: &roxmltree::Node, tag: &str) -> String {
    let color = extract_color_name(node);
    let size_hint = extract_size_hint(node);

    match tag {
        "text" => {
            let text: String = node.descendants()
                .filter(|n| n.is_text())
                .map(|n| n.text().unwrap_or(""))
                .collect::<String>()
                .trim()
                .to_string();
            if text.is_empty() {
                "Text".to_string()
            } else {
                let truncated = if text.len() > 24 { format!("{}...", &text[..21]) } else { text };
                format!("\"{}\"", truncated)
            }
        }
        "image" => {
            // Try to get filename from href
            let href = node.attribute("href")
                .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
                .unwrap_or("");
            if href.starts_with("data:") {
                format!("{} Image", size_hint)
            } else {
                let name = href.rsplit('/').next().unwrap_or("Image");
                name.to_string()
            }
        }
        "g" => {
            let child_count = node.children().filter(|c| c.is_element()).count();
            if child_count == 0 {
                "Empty Group".to_string()
            } else {
                format!("Group ({})", child_count)
            }
        }
        _ => {
            // Shape elements: combine color + tag + size
            let tag_label = match tag {
                "rect" => "Rect",
                "circle" => "Circle",
                "ellipse" => "Ellipse",
                "path" => "Path",
                "line" => "Line",
                "polygon" => "Polygon",
                "polyline" => "Polyline",
                other => other,
            };

            let mut parts = Vec::new();
            if !color.is_empty() {
                parts.push(color);
            }
            parts.push(tag_label.to_string());
            if !size_hint.is_empty() && tag != "line" {
                parts.push(size_hint);
            }
            parts.join(" ")
        }
    }
}

/// Extract a human-readable color name from fill or stroke attributes.
fn extract_color_name(node: &roxmltree::Node) -> String {
    let fill = node.attribute("fill").unwrap_or("");
    let stroke = node.attribute("stroke").unwrap_or("");

    // Prefer fill, fall back to stroke
    let color_str = if !fill.is_empty() && fill != "none" { fill } else { stroke };
    if color_str.is_empty() || color_str == "none" {
        return String::new();
    }

    // Check style attribute too
    let style = node.attribute("style").unwrap_or("");
    let effective_color = if color_str == "none" || color_str.is_empty() {
        // Try to extract fill from style
        if let Some(start) = style.find("fill:") {
            let rest = &style[start + 5..];
            rest.split(';').next().unwrap_or("").trim()
        } else {
            color_str
        }
    } else {
        color_str
    };

    hex_to_color_name(effective_color)
}

/// Convert a hex color or named color to a readable name.
fn hex_to_color_name(color: &str) -> String {
    let c = color.trim().to_lowercase();

    // Already a CSS named color — capitalize it
    let named = [
        "red", "blue", "green", "yellow", "orange", "purple", "pink", "cyan",
        "white", "black", "gray", "grey", "brown", "navy", "teal", "lime",
        "magenta", "gold", "silver", "maroon", "olive", "coral", "salmon",
        "indigo", "violet", "turquoise", "tan", "crimson", "khaki",
    ];
    for name in &named {
        if c == *name {
            let mut s = name.to_string();
            s[..1].make_ascii_uppercase();
            return s;
        }
    }

    // Parse hex to approximate color name
    if c.starts_with('#') && (c.len() == 4 || c.len() == 7) {
        let (r, g, b) = if c.len() == 4 {
            let r = u8::from_str_radix(&c[1..2], 16).unwrap_or(0) * 17;
            let g = u8::from_str_radix(&c[2..3], 16).unwrap_or(0) * 17;
            let b = u8::from_str_radix(&c[3..4], 16).unwrap_or(0) * 17;
            (r, g, b)
        } else {
            let r = u8::from_str_radix(&c[1..3], 16).unwrap_or(0);
            let g = u8::from_str_radix(&c[3..5], 16).unwrap_or(0);
            let b = u8::from_str_radix(&c[5..7], 16).unwrap_or(0);
            (r, g, b)
        };

        return approximate_color_name(r, g, b);
    }

    // RGB function: rgb(r, g, b)
    if c.starts_with("rgb(") {
        let inner = c.trim_start_matches("rgb(").trim_end_matches(')');
        let parts: Vec<u8> = inner.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if parts.len() == 3 {
            return approximate_color_name(parts[0], parts[1], parts[2]);
        }
    }

    String::new()
}

/// Map RGB values to the nearest human-readable color name.
fn approximate_color_name(r: u8, g: u8, b: u8) -> String {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);

    // Near black/white/gray
    if max < 30 { return "Black".into(); }
    if min > 225 { return "White".into(); }
    if max - min < 20 {
        return if max > 180 { "Light Gray".into() }
        else if max > 90 { "Gray".into() }
        else { "Dark Gray".into() };
    }

    // Dominant channel → color name
    let (dr, dg, db) = (r as f32, g as f32, b as f32);
    if dr > dg * 1.5 && dr > db * 1.5 {
        if dr > 200.0 && dg > 100.0 { "Orange".into() }
        else if dr > 180.0 && db > 100.0 { "Pink".into() }
        else { "Red".into() }
    } else if dg > dr * 1.3 && dg > db * 1.3 {
        if dg > 200.0 && dr > 150.0 { "Yellow-Green".into() }
        else { "Green".into() }
    } else if db > dr * 1.3 && db > dg * 1.3 {
        if db > 200.0 && dr > 100.0 { "Purple".into() }
        else { "Blue".into() }
    } else if dr > 200.0 && dg > 200.0 && db < 100.0 {
        "Yellow".into()
    } else if dr < 100.0 && dg > 180.0 && db > 180.0 {
        "Cyan".into()
    } else if dr > 180.0 && dg < 100.0 && db > 180.0 {
        "Magenta".into()
    } else if dr > 150.0 && dg > 100.0 && db < 80.0 {
        "Orange".into()
    } else if dr > 100.0 && dg > 60.0 && db < 60.0 {
        "Brown".into()
    } else {
        String::new()
    }
}

/// Estimate size from width/height or radius attributes.
fn extract_size_hint(node: &roxmltree::Node) -> String {
    let w: f32 = node.attribute("width").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let h: f32 = node.attribute("height").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let r: f32 = node.attribute("r").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let rx: f32 = node.attribute("rx").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let ry: f32 = node.attribute("ry").and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let area = if w > 0.0 && h > 0.0 {
        w * h
    } else if r > 0.0 {
        std::f32::consts::PI * r * r
    } else if rx > 0.0 && ry > 0.0 {
        std::f32::consts::PI * rx * ry
    } else {
        return String::new();
    };

    if area > 200_000.0 { "Large".into() }
    else if area > 20_000.0 { "Medium".into() }
    else if area > 2_000.0 { "Small".into() }
    else { "Tiny".into() }
}
