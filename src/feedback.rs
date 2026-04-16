//! Feedback export for AI collaboration.
//! Packages selected elements, annotations, and instructions into a JSON file
//! that Claude can read to make precise SVG edits.

use std::path::{Path, PathBuf};
use egui::Pos2;

/// An annotation drawn on top of the SVG for visual communication.
#[derive(Clone, Debug)]
pub enum Annotation {
    Circle { center: Pos2, radius_x: f32, radius_y: f32 },
    Arrow { start: Pos2, end: Pos2 },
    Text { pos: Pos2, text: String },
}

/// Sub-tool selection for annotation mode.
#[derive(Clone, Copy, PartialEq)]
pub enum AnnotationTool {
    Circle,
    Arrow,
    Text,
}

/// Export feedback data as JSON string.
pub fn export_json(
    svg_file: &str,
    selected_ids: &[String],
    annotations: &[Annotation],
    instruction: &str,
) -> String {
    let mut json = String::from("{\n");

    json.push_str(&format!("  \"svg_file\": \"{}\",\n", escape_json(svg_file)));
    json.push_str(&format!("  \"timestamp\": \"{}\",\n", timestamp()));

    // Selected elements
    json.push_str("  \"selected_elements\": [");
    for (i, id) in selected_ids.iter().enumerate() {
        if i > 0 { json.push_str(", "); }
        json.push_str(&format!("\"{}\"", escape_json(id)));
    }
    json.push_str("],\n");

    // Annotations
    json.push_str("  \"annotations\": [\n");
    for (i, ann) in annotations.iter().enumerate() {
        if i > 0 { json.push_str(",\n"); }
        match ann {
            Annotation::Circle { center, radius_x, radius_y } => {
                json.push_str(&format!(
                    "    {{\"type\": \"circle\", \"cx\": {:.1}, \"cy\": {:.1}, \"rx\": {:.1}, \"ry\": {:.1}}}",
                    center.x, center.y, radius_x, radius_y
                ));
            }
            Annotation::Arrow { start, end } => {
                json.push_str(&format!(
                    "    {{\"type\": \"arrow\", \"x1\": {:.1}, \"y1\": {:.1}, \"x2\": {:.1}, \"y2\": {:.1}}}",
                    start.x, start.y, end.x, end.y
                ));
            }
            Annotation::Text { pos, text } => {
                json.push_str(&format!(
                    "    {{\"type\": \"text\", \"x\": {:.1}, \"y\": {:.1}, \"text\": \"{}\"}}",
                    pos.x, pos.y, escape_json(text)
                ));
            }
        }
    }
    json.push_str("\n  ],\n");

    // Instruction
    json.push_str(&format!("  \"instruction\": \"{}\"\n", escape_json(instruction)));
    json.push('}');

    json
}

/// Write feedback JSON and screenshot next to the SVG file.
pub fn write_feedback(svg_path: &Path, json: &str) -> Result<PathBuf, String> {
    let dir = svg_path.parent().unwrap_or(Path::new("."));
    let feedback_path = dir.join(".forge-feedback.json");
    std::fs::write(&feedback_path, json).map_err(|e| format!("Write error: {}", e))?;
    Ok(feedback_path)
}

/// Build a temporary SVG overlay string containing annotations for screenshot rendering.
pub fn annotations_to_svg_overlay(annotations: &[Annotation], selected_bboxes: &[(String, egui::Rect)], svg_width: f32, svg_height: f32) -> String {
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}">"#,
        svg_width, svg_height
    );

    // Selection highlights
    for (id, bbox) in selected_bboxes {
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="none" stroke="#4FC3F7" stroke-width="2" opacity="0.8"/>"##,
            bbox.min.x, bbox.min.y, bbox.width(), bbox.height()
        ));
        // Label
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="#4FC3F7" font-size="11" font-family="Arial,sans-serif">{}</text>"##,
            bbox.min.x, bbox.min.y - 4.0, escape_xml(id)
        ));
    }

    // Annotations
    for ann in annotations {
        match ann {
            Annotation::Circle { center, radius_x, radius_y } => {
                svg.push_str(&format!(
                    r##"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="none" stroke="#FF5040" stroke-width="2.5" stroke-dasharray="6 3"/>"##,
                    center.x, center.y, radius_x, radius_y
                ));
            }
            Annotation::Arrow { start, end } => {
                svg.push_str(&format!(
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#FF5040" stroke-width="2.5"/>"##,
                    start.x, start.y, end.x, end.y
                ));
            }
            Annotation::Text { pos, text } => {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" fill="#FF5040" font-size="14" font-family="Arial,sans-serif" font-weight="bold">{}</text>"##,
                    pos.x, pos.y, escape_xml(text)
                ));
            }
        }
    }

    svg.push_str("</svg>");
    svg
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"', "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-ish timestamp from epoch seconds
    let secs_per_day = 86400u64;
    let days = now / secs_per_day;
    let time_of_day = now % secs_per_day;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Approximate date from days since epoch (good enough for feedback files)
    let (year, month, day) = days_to_date(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hours, minutes, seconds)
}

fn days_to_date(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1;
    for md in &month_days {
        if days < *md as u64 { break; }
        days -= *md as u64;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
