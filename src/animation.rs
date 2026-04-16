/// SMIL animation parser and evaluator.
/// Extracts <animate>, <animateTransform>, <animateMotion>, <set> elements,
/// evaluates them at a given time t, and produces a modified SVG string
/// with animated attribute values baked in.

#[derive(Debug, Clone)]
pub struct SmilAnimation {
    pub parent_id: String,
    pub tag: String,           // "animate", "animateTransform", "set", etc.
    pub attribute_name: String,
    pub attribute_type: String, // "transform" for animateTransform
    pub transform_type: String, // "translate", "rotate", "scale" for animateTransform
    pub from: String,
    pub to: String,
    pub values: Vec<String>,   // for multi-step animations
    pub dur: f64,              // seconds
    pub begin: f64,            // seconds
    pub repeat_count: RepeatCount,
    pub fill: String,          // "freeze" or "remove"
    pub calc_mode: String,     // "linear", "discrete", "paced", "spline"
}

#[derive(Debug, Clone)]
pub enum RepeatCount {
    Definite(f64),
    Indefinite,
}

/// Parse all SMIL animations from SVG content.
pub fn parse_animations(svg: &str) -> Vec<SmilAnimation> {
    let doc = match roxmltree::Document::parse(svg) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut anims = Vec::new();
    parse_node_animations(&doc.root(), &mut anims, "");
    anims
}

fn parse_node_animations(node: &roxmltree::Node, anims: &mut Vec<SmilAnimation>, parent_id: &str) {
    for child in node.children() {
        if !child.is_element() { continue; }

        let tag = child.tag_name().name();
        let id = child.attribute("id").unwrap_or(parent_id);

        match tag {
            "animate" | "animateTransform" | "animateMotion" | "set" => {
                let anim = SmilAnimation {
                    parent_id: parent_id.to_string(),
                    tag: tag.to_string(),
                    attribute_name: child.attribute("attributeName").unwrap_or("").to_string(),
                    attribute_type: child.attribute("attributeType").unwrap_or("").to_string(),
                    transform_type: child.attribute("type").unwrap_or("").to_string(),
                    from: child.attribute("from").unwrap_or("").to_string(),
                    to: child.attribute("to").unwrap_or("").to_string(),
                    values: child.attribute("values")
                        .map(|v| v.split(';').map(|s| s.trim().to_string()).collect())
                        .unwrap_or_default(),
                    dur: parse_time(child.attribute("dur").unwrap_or("0s")),
                    begin: parse_time(child.attribute("begin").unwrap_or("0s")),
                    repeat_count: match child.attribute("repeatCount").unwrap_or("1") {
                        "indefinite" => RepeatCount::Indefinite,
                        s => RepeatCount::Definite(s.parse().unwrap_or(1.0)),
                    },
                    fill: child.attribute("fill").unwrap_or("remove").to_string(),
                    calc_mode: child.attribute("calcMode").unwrap_or("linear").to_string(),
                };
                anims.push(anim);
            }
            _ => {
                parse_node_animations(&child, anims, id);
            }
        }
    }
}

/// Get the total duration of all animations.
pub fn total_duration(anims: &[SmilAnimation]) -> f64 {
    let mut max = 0.0f64;
    for a in anims {
        let repeat = match a.repeat_count {
            RepeatCount::Definite(n) => n,
            RepeatCount::Indefinite => 10.0, // show 10 cycles for indefinite
        };
        let end = a.begin + a.dur * repeat;
        max = max.max(end);
    }
    max.max(1.0) // minimum 1 second
}

/// Evaluate all animations at time t and produce a modified SVG.
/// This "bakes" animated values into the SVG attributes so resvg renders them.
pub fn evaluate_at(svg: &str, anims: &[SmilAnimation], t: f64) -> String {
    let mut result = svg.to_string();

    for anim in anims {
        if anim.parent_id.is_empty() || anim.dur <= 0.0 { continue; }

        // Compute local time within the animation
        let local_t = t - anim.begin;
        if local_t < 0.0 { continue; } // not started yet

        let progress = match anim.repeat_count {
            RepeatCount::Indefinite => (local_t % anim.dur) / anim.dur,
            RepeatCount::Definite(n) => {
                let total = anim.dur * n;
                if local_t > total {
                    if anim.fill == "freeze" { 1.0 } // freeze at final value
                    else { continue; } // remove — don't apply
                } else {
                    (local_t % anim.dur) / anim.dur
                }
            }
        }
        .clamp(0.0, 1.0);

        match anim.tag.as_str() {
            "animate" => {
                if let Some(value) = interpolate_value(&anim.from, &anim.to, &anim.values, progress) {
                    result = crate::svg_edit::set_attribute(&result, &anim.parent_id, &anim.attribute_name, &value);
                }
            }
            "animateTransform" => {
                if let Some(value) = interpolate_transform(&anim.transform_type, &anim.from, &anim.to, &anim.values, progress) {
                    result = crate::svg_edit::set_attribute(&result, &anim.parent_id, "transform", &value);
                }
            }
            "set" => {
                if progress > 0.0 {
                    result = crate::svg_edit::set_attribute(&result, &anim.parent_id, &anim.attribute_name, &anim.to);
                }
            }
            _ => {}
        }
    }

    result
}

/// Detect if SVG has any animations (SMIL or CSS).
pub fn has_animations(svg: &str) -> bool {
    svg.contains("<animate") ||
    svg.contains("<animateTransform") ||
    svg.contains("<animateMotion") ||
    svg.contains("<set ") ||
    svg.contains("@keyframes") ||
    svg.contains("animation:")
}

// ─── Interpolation helpers ───────────────────────────────

fn interpolate_value(from: &str, to: &str, values: &[String], progress: f64) -> Option<String> {
    if !values.is_empty() {
        return interpolate_values_list(values, progress);
    }
    if from.is_empty() || to.is_empty() { return None; }

    // Try numeric interpolation
    if let (Ok(f), Ok(t)) = (from.parse::<f64>(), to.parse::<f64>()) {
        let v = f + (t - f) * progress;
        return Some(format!("{:.2}", v));
    }

    // Try multi-number interpolation (e.g. "0 0" -> "100 50")
    let from_nums: Vec<f64> = from.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    let to_nums: Vec<f64> = to.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    if from_nums.len() == to_nums.len() && !from_nums.is_empty() {
        let interp: Vec<String> = from_nums.iter().zip(to_nums.iter())
            .map(|(f, t)| format!("{:.2}", f + (t - f) * progress))
            .collect();
        return Some(interp.join(" "));
    }

    // Color interpolation (simple hex)
    if from.starts_with('#') && to.starts_with('#') {
        return interpolate_color(from, to, progress);
    }

    // Discrete: use 'to' if past halfway
    if progress >= 0.5 { Some(to.to_string()) } else { Some(from.to_string()) }
}

fn interpolate_values_list(values: &[String], progress: f64) -> Option<String> {
    if values.len() < 2 { return values.first().cloned(); }
    let segments = values.len() - 1;
    let scaled = progress * segments as f64;
    let idx = (scaled.floor() as usize).min(segments - 1);
    let local = scaled - idx as f64;
    interpolate_value(&values[idx], &values[idx + 1], &[], local)
}

fn interpolate_transform(ttype: &str, from: &str, to: &str, values: &[String], progress: f64) -> Option<String> {
    let interp = if !values.is_empty() {
        interpolate_values_list(values, progress)?
    } else {
        interpolate_value(from, to, &[], progress)?
    };

    match ttype {
        "translate" => Some(format!("translate({})", interp)),
        "rotate" => Some(format!("rotate({})", interp)),
        "scale" => Some(format!("scale({})", interp)),
        "skewX" => Some(format!("skewX({})", interp)),
        "skewY" => Some(format!("skewY({})", interp)),
        _ => Some(format!("{}({})", ttype, interp)),
    }
}

fn interpolate_color(from: &str, to: &str, progress: f64) -> Option<String> {
    let f = parse_hex_color(from)?;
    let t = parse_hex_color(to)?;
    let r = (f.0 as f64 + (t.0 as f64 - f.0 as f64) * progress) as u8;
    let g = (f.1 as f64 + (t.1 as f64 - f.1 as f64) * progress) as u8;
    let b = (f.2 as f64 + (t.2 as f64 - f.2 as f64) * progress) as u8;
    Some(format!("#{:02x}{:02x}{:02x}", r, g, b))
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some((r, g, b))
    } else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
        Some((r, g, b))
    } else { None }
}

fn parse_time(s: &str) -> f64 {
    let s = s.trim();
    if s.ends_with("ms") {
        s.trim_end_matches("ms").parse::<f64>().unwrap_or(0.0) / 1000.0
    } else if s.ends_with("min") {
        s.trim_end_matches("min").parse::<f64>().unwrap_or(0.0) * 60.0
    } else {
        s.trim_end_matches('s').parse::<f64>().unwrap_or(0.0)
    }
}
