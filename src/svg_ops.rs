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
        .or_else(|| if !id.is_empty() { Some(id.clone()) } else { None })
        .unwrap_or_else(|| {
            // Generate a descriptive name from element type + key attributes
            match tag.as_str() {
                "rect" => format!("Rect"),
                "circle" => format!("Circle"),
                "ellipse" => format!("Ellipse"),
                "path" => format!("Path"),
                "text" => {
                    // Try to get text content
                    let text: String = node.descendants()
                        .filter(|n| n.is_text())
                        .map(|n| n.text().unwrap_or(""))
                        .collect();
                    if text.is_empty() { "Text".to_string() }
                    else { format!("\"{}\"", &text[..text.len().min(20)]) }
                }
                "image" => "Image".to_string(),
                "line" => "Line".to_string(),
                "polygon" => "Polygon".to_string(),
                "polyline" => "Polyline".to_string(),
                "g" => "Group".to_string(),
                _ => tag.clone(),
            }
        });

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
