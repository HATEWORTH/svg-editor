use egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use crate::animation;
use crate::feedback::{self, Annotation, AnnotationTool};
use crate::path_data::{self, PathCmd, PointKind};
use crate::svg_edit;

const HANDLE_SIZE: f32 = 7.0;
const HANDLE_HIT: f32 = 10.0;
const ROTATE_DIST: f32 = 30.0;
const NODE_SIZE: f32 = 6.0;
const NODE_HIT: f32 = 9.0;
const ACCENT: Color32 = Color32::from_rgb(79, 195, 247);
const ACCENT_DIM: Color32 = Color32::from_rgb(79, 195, 247);
const CTRL_COLOR: Color32 = Color32::from_rgb(255, 167, 38);
const HANDLE_LINE: Color32 = Color32::from_rgba_premultiplied(255, 167, 38, 140);
/// Animation render throttle interval in milliseconds (~30fps).
const ANIM_RENDER_THROTTLE_MS: u128 = 33;

/// Cached shape attributes for non-path element node editing.
#[derive(Clone, Default)]
struct ShapeAttrs {
    vals: Vec<(String, f32)>, // (attr_name, value) pairs
}

#[derive(Clone, Copy, PartialEq)]
pub enum EditTool {
    Select,
    Node,
    Annotate,
}

#[derive(Clone, PartialEq)]
enum DragMode {
    None,
    Move,
    Pan,
    Resize(u8),  // handle index 0-7
    Rotate,
    NodeDrag(usize), // point index in node_points
}

pub struct CanvasState {
    pub zoom: f32,
    pub pan: Vec2,
    pub svg_content: String,
    pub svg_width: f32,
    pub svg_height: f32,
    texture: Option<egui::TextureHandle>,
    texture_dirty: bool,
    pub selected_element: Option<String>,
    pub selected_bbox: Option<Rect>,
    drag_mode: DragMode,
    drag_start_svg: Pos2,
    drag_start_translate: (f32, f32),
    drag_offset: Vec2,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
    pub svg_modified: bool,
    element_bboxes: Vec<(String, Rect)>,
    /// Length of SVG content when bboxes were last successfully computed.
    /// Used to detect if content changed when parse fails.
    bboxes_svg_len: usize,
    // Tool
    pub tool: EditTool,
    // Resize state
    resize_anchor: Pos2,
    resize_orig_bbox: Rect,
    resize_orig_matrix: svg_edit::Matrix,
    resize_preview_bbox: Option<Rect>,
    // Rotate state
    rotate_center: Pos2,
    rotate_start_angle: f32,
    rotate_delta: f32,
    rotate_orig_matrix: svg_edit::Matrix,
    // Node editing
    node_element_id: String,
    node_element_tag: String,
    node_cmds: Vec<PathCmd>,
    node_points: Vec<path_data::NodePoint>,
    node_element_matrix: svg_edit::Matrix,
    node_element_inv_matrix: Option<svg_edit::Matrix>,
    node_dirty: bool,
    /// For shapes: store raw attribute values so we can write back changes
    node_shape_attrs: ShapeAttrs,
    // Animation
    pub is_animated: bool,
    pub animations: Vec<animation::SmilAnimation>,
    pub anim_time: f64,
    pub anim_duration: f64,
    pub anim_playing: bool,
    anim_last_instant: Option<std::time::Instant>,
    anim_last_render: Option<std::time::Instant>,
    // Multi-select
    pub selected_elements: Vec<String>,
    // Annotations (drawn on top of SVG, not part of SVG file)
    pub annotations: Vec<Annotation>,
    pub annotation_tool: AnnotationTool,
    annotation_drag_start: Option<Pos2>,
    pub annotation_text_editing: bool,
    pub annotation_text_buffer: String,
    // Shared font database (loaded once at startup for text rendering)
    pub fontdb: std::sync::Arc<fontdb::Database>,
}

impl CanvasState {
    pub fn new() -> Self {
        let mut fdb = fontdb::Database::new();
        fdb.load_system_fonts();
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            svg_content: String::new(),
            svg_width: 800.0,
            svg_height: 600.0,
            texture: None,
            texture_dirty: true,
            selected_element: None,
            selected_bbox: None,
            drag_mode: DragMode::None,
            drag_start_svg: Pos2::ZERO,
            drag_start_translate: (0.0, 0.0),
            drag_offset: Vec2::ZERO,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            svg_modified: false,
            element_bboxes: Vec::new(),
            bboxes_svg_len: 0,
            tool: EditTool::Select,
            resize_anchor: Pos2::ZERO,
            resize_orig_bbox: Rect::NOTHING,
            resize_orig_matrix: svg_edit::identity(),
            resize_preview_bbox: None,
            rotate_center: Pos2::ZERO,
            rotate_start_angle: 0.0,
            rotate_delta: 0.0,
            rotate_orig_matrix: svg_edit::identity(),
            node_element_id: String::new(),
            node_element_tag: String::new(),
            node_cmds: Vec::new(),
            node_points: Vec::new(),
            node_element_matrix: svg_edit::identity(),
            node_element_inv_matrix: None,
            node_dirty: false,
            node_shape_attrs: ShapeAttrs::default(),
            is_animated: false,
            animations: Vec::new(),
            anim_time: 0.0,
            anim_duration: 0.0,
            anim_playing: false,
            anim_last_instant: None,
            anim_last_render: None,
            selected_elements: Vec::new(),
            annotations: Vec::new(),
            annotation_tool: AnnotationTool::Circle,
            annotation_drag_start: None,
            annotation_text_editing: false,
            annotation_text_buffer: String::new(),
            fontdb: std::sync::Arc::new(fdb),
        }
    }

    pub fn load_svg(&mut self, content: String) {
        self.svg_content = svg_edit::auto_assign_ids(&content);
        self.texture_dirty = true;
        self.drag_mode = DragMode::None;
        self.drag_offset = Vec2::ZERO;
        self.parse_dimensions();
        self.is_animated = animation::has_animations(&self.svg_content);
        if self.is_animated {
            self.animations = animation::parse_animations(&self.svg_content);
            self.anim_duration = animation::total_duration(&self.animations);
            self.anim_time = 0.0;
            self.anim_playing = false;
            self.anim_last_instant = None;
        } else {
            self.animations.clear();
            self.anim_duration = 0.0;
            self.anim_playing = false;
        }
        self.rebuild_bboxes();
        self.clear_node_data();
    }

    pub fn load_svg_with_undo(&mut self, content: String) {
        self.push_undo();
        self.load_svg(content);
    }

    fn parse_dimensions(&mut self) {
        // Reset to default dimensions first (prevents inheriting from previous file)
        self.svg_width = 800.0;
        self.svg_height = 600.0;

        if let Ok(doc) = roxmltree::Document::parse(&self.svg_content) {
            let root = doc.root_element();

            // Try viewBox first (most reliable)
            if let Some(vb) = root.attribute("viewBox") {
                let p: Vec<f32> = vb.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                if p.len() == 4 && p[2] > 0.0 && p[3] > 0.0 {
                    self.svg_width = p[2];
                    self.svg_height = p[3];
                    return;
                }
            }

            // Fallback to width/height attributes (skip percentages)
            let w: Option<f32> = root.attribute("width")
                .filter(|s| !s.contains('%'))
                .and_then(|s| s.trim_end_matches("px").parse().ok());
            let h: Option<f32> = root.attribute("height")
                .filter(|s| !s.contains('%'))
                .and_then(|s| s.trim_end_matches("px").parse().ok());
            if let (Some(w), Some(h)) = (w, h) {
                if w > 0.0 && h > 0.0 {
                    self.svg_width = w;
                    self.svg_height = h;
                }
            }
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.svg_content.clone());
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.svg_content.clone());
            self.svg_content = prev;
            self.texture_dirty = true;
            self.rebuild_bboxes();
            self.svg_modified = true;
            self.update_selected_bbox();
            self.refresh_node_data();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.svg_content.clone());
            self.svg_content = next;
            self.texture_dirty = true;
            self.rebuild_bboxes();
            self.svg_modified = true;
            self.update_selected_bbox();
            self.refresh_node_data();
            true
        } else {
            false
        }
    }

    fn commit(&mut self, new_svg: String) {
        self.push_undo();
        self.svg_content = new_svg;
        self.texture_dirty = true;
        self.svg_modified = true;
        self.rebuild_bboxes();
        self.update_selected_bbox();
    }

    fn update_selected_bbox(&mut self) {
        if let Some(ref id) = self.selected_element {
            self.selected_bbox = self.element_bboxes.iter()
                .find(|(eid, _)| eid == id)
                .map(|(_, b)| *b);
        }
    }

    fn rebuild_bboxes(&mut self) {
        if self.svg_content.is_empty() {
            self.element_bboxes.clear();
            self.bboxes_svg_len = 0;
            return;
        }
        let opt = usvg::Options { fontdb: self.fontdb.clone(), ..Default::default() };
        if let Ok(tree) = usvg::Tree::from_str(&self.svg_content, &opt) {
            let mut raw = Vec::new();
            collect_bboxes_ordered(&mut raw, tree.root(), usvg::Transform::identity());
            let mut seen = std::collections::HashSet::new();
            let mut deduped = Vec::new();
            for (id, bbox) in raw.into_iter().rev() {
                if seen.insert(id.clone()) {
                    deduped.push((id, bbox));
                }
            }
            deduped.reverse();
            self.element_bboxes = deduped;
            self.bboxes_svg_len = self.svg_content.len();
        } else if self.svg_content.len() != self.bboxes_svg_len {
            // Content changed but parse failed - clear stale bboxes to avoid
            // misaligned selection handles pointing to outdated coordinates
            self.element_bboxes.clear();
            self.bboxes_svg_len = 0;
        }
        // If content unchanged and parse fails, keep previous bboxes (transient error)
    }

    pub fn fit_to_view(&mut self, s: Vec2) {
        let p = 40.0;
        let sx = (s.x - p * 2.0) / self.svg_width;
        let sy = (s.y - p * 2.0) / self.svg_height;
        self.zoom = sx.min(sy).min(10.0);
        self.pan = Vec2::new(
            (s.x - self.svg_width * self.zoom) / 2.0,
            (s.y - self.svg_height * self.zoom) / 2.0,
        );
        self.texture_dirty = true;
    }

    /// Viewport rendering: rasterize only the visible SVG region at native
    /// screen pixel resolution. This keeps the image crisp at any zoom level
    /// without creating enormous textures.
    fn ensure_texture(&mut self, ctx: &egui::Context, canvas_rect: Rect) {
        if !self.texture_dirty && self.texture.is_some() {
            return;
        }
        // Skip re-render if canvas rect is too small (e.g. dialog overlay covering canvas).
        // This prevents SVG stretching when dialogs reduce available space. We intentionally
        // leave texture_dirty=true so rendering resumes when the dialog closes and space returns.
        if canvas_rect.width() < 100.0 || canvas_rect.height() < 100.0 {
            return;
        }
        self.texture_dirty = false;
        if self.svg_content.is_empty() {
            return;
        }

        let render_svg = if self.is_animated && !self.animations.is_empty() {
            animation::evaluate_at(&self.svg_content, &self.animations, self.anim_time)
        } else {
            self.svg_content.clone()
        };

        let opt = usvg::Options { fontdb: self.fontdb.clone(), ..Default::default() };
        let tree = match usvg::Tree::from_str(&render_svg, &opt) {
            Ok(t) => t,
            Err(_) => return, // Keep existing texture on parse failure
        };

        // Pixmap = canvas area at native screen resolution (DPI-aware)
        let dpi = ctx.pixels_per_point();
        let rw = (canvas_rect.width() * dpi).ceil() as u32;
        let rh = (canvas_rect.height() * dpi).ceil() as u32;
        if rw == 0 || rh == 0 {
            return;
        }

        let mut px = match tiny_skia::Pixmap::new(rw, rh) {
            Some(p) => p,
            None => return,
        };
        // Transparent background — the document white rect and grid are drawn
        // separately by the painter, so the SVG renders with alpha.

        // Transform maps SVG coordinates directly to pixmap coordinates:
        //   pixmap_x = svg_x * zoom * dpi + pan_x * dpi
        //   pixmap_y = svg_y * zoom * dpi + pan_y * dpi
        // resvg only renders what falls inside the pixmap bounds.
        let scale = self.zoom * dpi;
        let tf = tiny_skia::Transform::from_scale(scale, scale)
            .post_translate(self.pan.x * dpi, self.pan.y * dpi);

        resvg::render(&tree, tf, &mut px.as_mut());

        // Convert tiny-skia premultiplied RGBA to egui straight RGBA (optimized)
        let d = px.data_mut();
        for c in d.chunks_exact_mut(4) {
            let a = c[3];
            if a > 0 && a < 255 {
                let inv_a = 255.0 / a as f32;
                c[0] = (c[0] as f32 * inv_a).min(255.0) as u8;
                c[1] = (c[1] as f32 * inv_a).min(255.0) as u8;
                c[2] = (c[2] as f32 * inv_a).min(255.0) as u8;
            }
            // a == 0: leave as [0,0,0,0]; a == 255: already straight
        }
        let rgba = d;
        let img = egui::ColorImage::from_rgba_unmultiplied([rw as usize, rh as usize], &rgba);
        self.texture = Some(ctx.load_texture("svg-canvas", img, egui::TextureOptions::LINEAR));
    }

    pub fn tick_animation(&mut self) -> bool {
        if !self.anim_playing || !self.is_animated {
            return false;
        }
        let now = std::time::Instant::now();
        if let Some(last) = self.anim_last_instant {
            let dt = now.duration_since(last).as_secs_f64();
            self.anim_time += dt;
            if self.anim_duration > 0.0 && self.anim_time > self.anim_duration {
                self.anim_time = self.anim_time % self.anim_duration;
            }
            // Throttle rendering to avoid expensive re-parse every frame
            let since_render = now.duration_since(self.anim_last_render.unwrap_or(now));
            if since_render.as_millis() >= ANIM_RENDER_THROTTLE_MS || self.anim_last_render.is_none() {
                self.texture_dirty = true;
                self.anim_last_render = Some(now);
            }
        }
        self.anim_last_instant = Some(now);
        true
    }

    pub fn play_pause(&mut self) {
        self.anim_playing = !self.anim_playing;
        if self.anim_playing {
            self.anim_last_instant = Some(std::time::Instant::now());
        } else {
            self.anim_last_instant = None;
        }
    }

    pub fn seek(&mut self, t: f64) {
        self.anim_time = t.clamp(0.0, self.anim_duration);
        self.texture_dirty = true;
    }

    // SVG coord ↔ screen coord
    fn s2s(&self, p: Pos2, c: Pos2) -> Pos2 {
        Pos2::new(c.x + self.pan.x + p.x * self.zoom, c.y + self.pan.y + p.y * self.zoom)
    }
    fn s2v(&self, p: Pos2, c: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - c.x - self.pan.x) / self.zoom,
            (p.y - c.y - self.pan.y) / self.zoom,
        )
    }

    // ─── Node editing helpers ───────────────────────────

    fn clear_node_data(&mut self) {
        self.node_element_id.clear();
        self.node_element_tag.clear();
        self.node_cmds.clear();
        self.node_points.clear();
        self.node_element_matrix = svg_edit::identity();
        self.node_element_inv_matrix = None;
        self.node_dirty = false;
        self.node_shape_attrs = ShapeAttrs::default();
    }

    fn needs_node_refresh(&self) -> bool {
        if self.tool != EditTool::Node {
            return false;
        }
        match &self.selected_element {
            Some(id) => self.node_element_id != *id,
            None => !self.node_element_id.is_empty(),
        }
    }

    fn refresh_node_data(&mut self) {
        if self.tool != EditTool::Node {
            self.clear_node_data();
            return;
        }
        let id = match &self.selected_element {
            Some(id) => id.clone(),
            None => {
                self.clear_node_data();
                return;
            }
        };

        let tag = svg_edit::get_element_tag(&self.svg_content, &id)
            .unwrap_or_default();

        // Load element transform
        let transform_str = svg_edit::get_attribute(&self.svg_content, &id, "transform")
            .unwrap_or_default();
        self.node_element_matrix = svg_edit::parse_transform(&transform_str);
        self.node_element_inv_matrix = svg_edit::inverse(&self.node_element_matrix);
        self.node_element_id = id.clone();
        self.node_element_tag = tag.clone();
        self.node_shape_attrs = ShapeAttrs::default();

        match tag.as_str() {
            "path" => {
                if let Some(d) = svg_edit::get_attribute(&self.svg_content, &id, "d") {
                    self.node_cmds = path_data::parse(&d);
                    self.node_points = path_data::extract_points(&self.node_cmds);
                }
            }
            "rect" => {
                let x = self.attr_f(&id, "x");
                let y = self.attr_f(&id, "y");
                let w = self.attr_f(&id, "width");
                let h = self.attr_f(&id, "height");
                let rx = self.attr_f(&id, "rx").min(w / 2.0);
                let ry_raw = svg_edit::get_attribute(&self.svg_content, &id, "ry");
                let ry = ry_raw.and_then(|s| s.parse::<f32>().ok()).unwrap_or(rx).min(h / 2.0);
                self.node_shape_attrs.vals = vec![
                    ("x".into(), x), ("y".into(), y),
                    ("width".into(), w), ("height".into(), h),
                    ("rx".into(), rx), ("ry".into(), ry),
                ];
                // 4 corner anchor points
                self.node_points = vec![
                    path_data::NodePoint { pos: Pos2::new(x, y), kind: PointKind::Anchor, cmd_idx: 0, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(x + w, y), kind: PointKind::Anchor, cmd_idx: 1, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(x + w, y + h), kind: PointKind::Anchor, cmd_idx: 2, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(x, y + h), kind: PointKind::Anchor, cmd_idx: 3, field: 0 },
                ];
                // Build outline with rounded corners using cubic bezier arcs
                self.node_cmds = build_rounded_rect_cmds(x, y, w, h, rx, ry);
            }
            "circle" => {
                let cx = self.attr_f(&id, "cx");
                let cy = self.attr_f(&id, "cy");
                let r = self.attr_f(&id, "r");
                self.node_shape_attrs.vals = vec![
                    ("cx".into(), cx), ("cy".into(), cy), ("r".into(), r),
                ];
                // 4 cardinal anchor points
                self.node_points = vec![
                    path_data::NodePoint { pos: Pos2::new(cx, cy - r), kind: PointKind::Anchor, cmd_idx: 0, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(cx + r, cy), kind: PointKind::Anchor, cmd_idx: 1, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(cx, cy + r), kind: PointKind::Anchor, cmd_idx: 2, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(cx - r, cy), kind: PointKind::Anchor, cmd_idx: 3, field: 0 },
                ];
                // Approximate circle with 4 cubic bezier arcs
                self.node_cmds = build_ellipse_cmds(cx, cy, r, r);
            }
            "ellipse" => {
                let cx = self.attr_f(&id, "cx");
                let cy = self.attr_f(&id, "cy");
                let rx = self.attr_f(&id, "rx");
                let ry = self.attr_f(&id, "ry");
                self.node_shape_attrs.vals = vec![
                    ("cx".into(), cx), ("cy".into(), cy),
                    ("rx".into(), rx), ("ry".into(), ry),
                ];
                self.node_points = vec![
                    path_data::NodePoint { pos: Pos2::new(cx, cy - ry), kind: PointKind::Anchor, cmd_idx: 0, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(cx + rx, cy), kind: PointKind::Anchor, cmd_idx: 1, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(cx, cy + ry), kind: PointKind::Anchor, cmd_idx: 2, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(cx - rx, cy), kind: PointKind::Anchor, cmd_idx: 3, field: 0 },
                ];
                self.node_cmds = build_ellipse_cmds(cx, cy, rx, ry);
            }
            "line" => {
                let x1 = self.attr_f(&id, "x1");
                let y1 = self.attr_f(&id, "y1");
                let x2 = self.attr_f(&id, "x2");
                let y2 = self.attr_f(&id, "y2");
                self.node_shape_attrs.vals = vec![
                    ("x1".into(), x1), ("y1".into(), y1),
                    ("x2".into(), x2), ("y2".into(), y2),
                ];
                self.node_points = vec![
                    path_data::NodePoint { pos: Pos2::new(x1, y1), kind: PointKind::Anchor, cmd_idx: 0, field: 0 },
                    path_data::NodePoint { pos: Pos2::new(x2, y2), kind: PointKind::Anchor, cmd_idx: 1, field: 0 },
                ];
                self.node_cmds = vec![
                    PathCmd::Move(x1, y1),
                    PathCmd::Line(x2, y2),
                ];
            }
            "polygon" | "polyline" => {
                if let Some(pts_str) = svg_edit::get_attribute(&self.svg_content, &id, "points") {
                    let coords: Vec<f32> = pts_str
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|s| !s.is_empty())
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    let mut cmds = Vec::new();
                    let mut points = Vec::new();
                    for (i, pair) in coords.chunks(2).enumerate() {
                        if pair.len() == 2 {
                            let (x, y) = (pair[0], pair[1]);
                            if i == 0 {
                                cmds.push(PathCmd::Move(x, y));
                            } else {
                                cmds.push(PathCmd::Line(x, y));
                            }
                            points.push(path_data::NodePoint {
                                pos: Pos2::new(x, y),
                                kind: PointKind::Anchor,
                                cmd_idx: i,
                                field: 0,
                            });
                        }
                    }
                    if tag == "polygon" {
                        cmds.push(PathCmd::Close);
                    }
                    self.node_cmds = cmds;
                    self.node_points = points;
                }
            }
            _ => {
                self.node_cmds.clear();
                self.node_points.clear();
            }
        }
    }

    fn attr_f(&self, id: &str, attr: &str) -> f32 {
        svg_edit::get_attribute(&self.svg_content, id, attr)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_mode != DragMode::None
    }

    // ─── Main show ──────────────────────────────────────

    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let cr = response.rect;

        self.ensure_texture(ctx, cr);

        draw_dot_grid(&painter, cr);

        // Document background: white rect at SVG bounds with shadow
        {
            let sr = Rect::from_min_size(
                Pos2::new(cr.min.x + self.pan.x, cr.min.y + self.pan.y),
                Vec2::new(self.svg_width * self.zoom, self.svg_height * self.zoom),
            );
            painter.rect_filled(
                sr.translate(Vec2::new(3.0, 3.0)),
                4.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 60),
            );
            painter.rect_filled(sr, 0.0, Color32::WHITE);
        }

        // Draw SVG content — viewport-rendered texture fills the canvas area
        if let Some(tex) = &self.texture {
            painter.image(
                tex.id(),
                cr,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        let hover = ui.input(|i| i.pointer.hover_pos());

        // ─── Zoom (both tools) ──────────────────────
        if let Some(m) = hover {
            if cr.contains(m) {
                let sc = ui.input(|i| i.smooth_scroll_delta.y);
                if sc != 0.0 {
                    let f = if sc > 0.0 { 1.1f32 } else { 0.9f32 };
                    let nz = (self.zoom * f).clamp(0.05, 20.0);
                    let s = nz / self.zoom;
                    let mx = m.x - cr.min.x;
                    let my = m.y - cr.min.y;
                    self.pan.x = mx - s * (mx - self.pan.x);
                    self.pan.y = my - s * (my - self.pan.y);
                    self.zoom = nz;
                    self.texture_dirty = true;
                }
            }
        }

        // Middle/right drag = pan (both tools)
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = response.drag_delta();
            if delta.length() > 0.0 {
                self.pan += delta;
                self.texture_dirty = true;
            }
        }

        match self.tool {
            EditTool::Select => self.show_select_tool(&response, &painter, cr, hover, ctx),
            EditTool::Node => self.show_node_tool(&response, &painter, cr, hover),
            EditTool::Annotate => self.show_annotate_tool(&response, &painter, cr, hover, ctx),
        }

        // Draw annotations on top in ALL tool modes
        self.draw_annotations(&painter, cr, hover);

        // Draw multi-select highlights (secondary selections)
        for sel_id in &self.selected_elements.clone() {
            if self.selected_element.as_ref() == Some(sel_id) { continue; } // primary already drawn
            if let Some((_, bbox)) = self.element_bboxes.iter().find(|(id, _)| id == sel_id) {
                let min = self.s2s(bbox.min, cr.min);
                let max = self.s2s(bbox.max, cr.min);
                let sr = Rect::from_min_max(min, max).expand(2.0);
                painter.rect_stroke(sr, 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(79, 195, 247, 120)));
            }
        }
    }

    // ─── SELECT TOOL ────────────────────────────────────

    fn show_select_tool(
        &mut self,
        response: &egui::Response,
        painter: &egui::Painter,
        cr: Rect,
        hover: Option<Pos2>,
        ctx: &egui::Context,
    ) {
        // Draw selection overlay
        if let Some(bbox) = self.selected_bbox {
            let (draw_bbox, rot_angle) = match &self.drag_mode {
                DragMode::Resize(_) => {
                    (self.resize_preview_bbox.unwrap_or(bbox), 0.0)
                }
                DragMode::Rotate => (bbox, self.rotate_delta),
                DragMode::Move => {
                    let moved = bbox.translate(self.drag_offset);
                    (moved, 0.0)
                }
                _ => (bbox, 0.0),
            };

            if rot_angle.abs() > 0.001 {
                // Draw rotated selection rectangle
                self.draw_rotated_selection(painter, cr, draw_bbox, rot_angle);
            } else {
                // Normal axis-aligned selection
                let min = self.s2s(draw_bbox.min, cr.min);
                let max = self.s2s(draw_bbox.max, cr.min);
                let sr = Rect::from_min_max(min, max).expand(2.0);

                painter.rect_stroke(sr, 0.0, Stroke::new(1.5, ACCENT));

                // 8 resize handles
                for h in resize_handles(sr) {
                    painter.rect_filled(
                        Rect::from_center_size(h, Vec2::splat(HANDLE_SIZE)),
                        1.0,
                        Color32::WHITE,
                    );
                    painter.rect_stroke(
                        Rect::from_center_size(h, Vec2::splat(HANDLE_SIZE)),
                        1.0,
                        Stroke::new(1.0, ACCENT),
                    );
                }

                // Rotate handle
                let rp = Pos2::new(sr.center().x, sr.min.y - ROTATE_DIST);
                painter.line_segment(
                    [Pos2::new(sr.center().x, sr.min.y), rp],
                    Stroke::new(1.0, ACCENT),
                );
                painter.circle_filled(rp, 5.0, ACCENT);
            }

            // Element label
            if self.drag_mode == DragMode::None {
                if let Some(ref id) = self.selected_element {
                    let label_pos = self.s2s(
                        Pos2::new(draw_bbox.min.x, draw_bbox.min.y),
                        cr.min,
                    );
                    painter.text(
                        Pos2::new(label_pos.x, label_pos.y - 16.0),
                        egui::Align2::LEFT_BOTTOM,
                        id,
                        egui::FontId::proportional(11.0),
                        ACCENT,
                    );
                }
            }

            // Show rotation angle during rotate
            if self.drag_mode == DragMode::Rotate {
                if let Some(pos) = hover {
                    let angle_deg = self.rotate_delta.to_degrees();
                    painter.text(
                        Pos2::new(pos.x + 16.0, pos.y - 8.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{:.1}\u{00B0}", angle_deg),
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                }
            }
        }

        // ─── Input handling ─────────────────────────

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = hover {
                let svp = self.s2v(pos, cr.min);

                if let Some(bbox) = self.selected_bbox {
                    let min = self.s2s(bbox.min, cr.min);
                    let max = self.s2s(bbox.max, cr.min);
                    let sr = Rect::from_min_max(min, max).expand(2.0);

                    // Hit test rotate handle
                    let rp = Pos2::new(sr.center().x, sr.min.y - ROTATE_DIST);
                    if pos.distance(rp) <= HANDLE_HIT {
                        self.start_rotate(bbox, svp);
                        return;
                    }

                    // Hit test resize handles
                    let handles = resize_handles(sr);
                    for (idx, h) in handles.iter().enumerate() {
                        if pos.distance(*h) <= HANDLE_HIT {
                            self.start_resize(idx as u8, bbox);
                            return;
                        }
                    }
                }

                // Try select / start move (with multi-select support)
                let ctrl = ctx.input(|i| i.modifiers.ctrl);
                let shift = ctx.input(|i| i.modifiers.shift);
                self.try_select_multi(svp, ctrl, shift);
                if self.selected_element.is_some() && !ctrl && !shift {
                    if let Some(ref id) = self.selected_element {
                        self.drag_start_translate = svg_edit::get_translate(&self.svg_content, id);
                    }
                    self.drag_mode = DragMode::Move;
                    self.drag_start_svg = svp;
                    self.drag_offset = Vec2::ZERO;
                }
            }
        }

        // Drag in progress
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = hover {
                let svp = self.s2v(pos, cr.min);
                match &self.drag_mode {
                    DragMode::Move => {
                        self.drag_offset = Vec2::new(
                            svp.x - self.drag_start_svg.x,
                            svp.y - self.drag_start_svg.y,
                        );
                    }
                    DragMode::Resize(handle) => {
                        let handle = *handle;
                        self.update_resize(handle, svp);
                    }
                    DragMode::Rotate => {
                        self.update_rotate(svp);
                    }
                    _ => {}
                }
            }
        }

        // Drag end
        if response.drag_stopped() {
            match self.drag_mode.clone() {
                DragMode::Move => {
                    if let Some(ref sel_id) = self.selected_element.clone() {
                        if self.drag_offset.length() > 0.5 {
                            let (sx, sy) = self.drag_start_translate;
                            let ns = svg_edit::set_translate(
                                &self.svg_content,
                                sel_id,
                                sx + self.drag_offset.x,
                                sy + self.drag_offset.y,
                            );
                            let moved_bbox = self.selected_bbox.map(|b| b.translate(self.drag_offset));
                            self.commit(ns);
                            self.selected_bbox = moved_bbox;
                        }
                    }
                    self.drag_mode = DragMode::None;
                    self.drag_offset = Vec2::ZERO;
                }
                DragMode::Resize(_) => {
                    self.commit_resize();
                    self.drag_mode = DragMode::None;
                    self.resize_preview_bbox = None;
                }
                DragMode::Rotate => {
                    self.commit_rotate();
                    self.drag_mode = DragMode::None;
                    self.rotate_delta = 0.0;
                }
                _ => {
                    self.drag_mode = DragMode::None;
                }
            }
        } else if response.clicked() && self.drag_mode == DragMode::None {
            if let Some(pos) = hover {
                let svp = self.s2v(pos, cr.min);
                let ctrl = ctx.input(|i| i.modifiers.ctrl);
                let shift = ctx.input(|i| i.modifiers.shift);
                self.try_select_multi(svp, ctrl, shift);
            }
        }
    }

    // ─── NODE TOOL ──────────────────────────────────────

    fn show_node_tool(
        &mut self,
        response: &egui::Response,
        painter: &egui::Painter,
        cr: Rect,
        hover: Option<Pos2>,
    ) {
        // Load node data if selection changed or data is stale
        if self.needs_node_refresh() {
            self.refresh_node_data();
        }

        // Draw the path outline using painter primitives
        if !self.node_cmds.is_empty() {
            self.draw_path_outline(painter, cr);
        }

        // Draw node points and control handles
        if !self.node_points.is_empty() {
            self.draw_node_points(painter, cr, hover);
        }

        // Draw selection bbox (subtle, no handles)
        if let Some(bbox) = self.selected_bbox {
            let min = self.s2s(bbox.min, cr.min);
            let max = self.s2s(bbox.max, cr.min);
            let sr = Rect::from_min_max(min, max).expand(2.0);
            painter.rect_stroke(
                sr,
                0.0,
                Stroke::new(0.5, Color32::from_rgba_unmultiplied(79, 195, 247, 60)),
            );
        }

        // Debug overlay: show node info (only in debug builds)
        #[cfg(debug_assertions)]
        if let Some(ref id) = self.selected_element {
            let info = format!(
                "id={} tag={} pts={} cmds={} dirty={}",
                id, self.node_element_tag, self.node_points.len(), self.node_cmds.len(), self.node_dirty
            );
            painter.text(
                Pos2::new(cr.min.x + 10.0, cr.min.y + 20.0),
                egui::Align2::LEFT_TOP,
                &info,
                egui::FontId::monospace(12.0),
                Color32::YELLOW,
            );
            for (i, pt) in self.node_points.iter().take(4).enumerate() {
                let pt_info = format!("  pt{}: ({:.0},{:.0}) {:?}", i, pt.pos.x, pt.pos.y, pt.kind);
                painter.text(
                    Pos2::new(cr.min.x + 10.0, cr.min.y + 36.0 + i as f32 * 14.0),
                    egui::Align2::LEFT_TOP,
                    &pt_info,
                    egui::FontId::monospace(11.0),
                    Color32::YELLOW,
                );
            }
        }

        // ─── Input ──────────────────────────────────

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = hover {
                let svp = self.s2v(pos, cr.min);

                // Hit test node points
                if let Some(hit_idx) = self.hit_test_node(pos, cr) {
                    self.push_undo(); // save state once before drag begins
                    self.drag_mode = DragMode::NodeDrag(hit_idx);
                    self.drag_start_svg = svp;
                    return;
                }

                // Try to select a different element
                self.try_select(svp);
                self.refresh_node_data();
            }
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = hover {
                let svp = self.s2v(pos, cr.min);
                if let DragMode::NodeDrag(idx) = self.drag_mode {
                    // Update point position and immediately apply to SVG
                    self.update_node_drag(idx, svp);
                    self.apply_node_changes_live();
                }
            }
        }

        if response.drag_stopped() {
            if let DragMode::NodeDrag(_) = self.drag_mode {
                // Final commit — rebuild bboxes for the finished state
                self.rebuild_bboxes();
                self.update_selected_bbox();
                self.node_dirty = false;
            }
            self.drag_mode = DragMode::None;
        } else if response.clicked() && self.drag_mode == DragMode::None {
            if let Some(pos) = hover {
                let svp = self.s2v(pos, cr.min);
                if self.hit_test_node(pos, cr).is_none() {
                    self.try_select(svp);
                    self.refresh_node_data();
                }
            }
        }
    }

    // ─── Selection ──────────────────────────────────────

    fn try_select(&mut self, svg_pos: Pos2) {
        self.try_select_multi(svg_pos, false, false);
    }

    fn try_select_multi(&mut self, svg_pos: Pos2, ctrl: bool, shift: bool) {
        // Find the topmost element under the cursor
        let hit = self.element_bboxes.iter().rev()
            .find(|(_, bbox)| bbox.contains(svg_pos))
            .map(|(id, bbox)| (id.clone(), *bbox));

        if let Some((id, bbox)) = hit {
            if ctrl || shift {
                // Toggle in/out of multi-selection
                if let Some(pos) = self.selected_elements.iter().position(|s| s == &id) {
                    self.selected_elements.remove(pos);
                    // If we removed the primary, pick another or clear
                    if self.selected_element.as_ref() == Some(&id) {
                        self.selected_element = self.selected_elements.last().cloned();
                        self.selected_bbox = self.selected_element.as_ref().and_then(|sel_id| {
                            self.element_bboxes.iter().find(|(eid, _)| eid == sel_id).map(|(_, b)| *b)
                        });
                    }
                } else {
                    self.selected_elements.push(id.clone());
                    self.selected_element = Some(id);
                    self.selected_bbox = Some(bbox);
                }
            } else {
                // Normal click — single select, clear multi
                self.selected_elements.clear();
                self.selected_elements.push(id.clone());
                self.selected_element = Some(id);
                self.selected_bbox = Some(bbox);
            }
        } else if !ctrl && !shift {
            // Click on empty space — deselect all
            self.selected_element = None;
            self.selected_bbox = None;
            self.selected_elements.clear();
        }
    }

    pub fn clear_annotations(&mut self) {
        self.annotations.clear();
        self.annotation_text_editing = false;
        self.annotation_text_buffer.clear();
    }

    pub fn is_editing_annotation_text(&self) -> bool {
        self.annotation_text_editing
    }

    /// Get selected element bboxes for feedback export.
    pub fn selected_bboxes(&self) -> Vec<(String, Rect)> {
        self.selected_elements.iter()
            .filter_map(|id| {
                self.element_bboxes.iter()
                    .find(|(eid, _)| eid == id)
                    .map(|(eid, bbox)| (eid.clone(), *bbox))
            })
            .collect()
    }

    /// Capture a screenshot PNG with annotations overlaid.
    pub fn save_screenshot_png(&self, path: &std::path::Path) -> Result<(), String> {
        if self.svg_content.is_empty() { return Err("No SVG content".into()); }

        // Build SVG with annotation overlay
        let overlay = feedback::annotations_to_svg_overlay(
            &self.annotations,
            &self.selected_bboxes(),
            self.svg_width,
            self.svg_height,
        );

        // Combine: insert overlay content before closing </svg>
        let mut combined = self.svg_content.clone();
        if let Some(pos) = combined.rfind("</svg>") {
            // Extract overlay inner content (between <svg...> and </svg>)
            if let (Some(start), Some(end)) = (overlay.find('>'), overlay.rfind("</svg>")) {
                let inner = &overlay[start + 1..end];
                let group = format!("<g opacity=\"0.9\">{}</g>\n", inner);
                combined.insert_str(pos, &group);
            }
        }

        let opt = usvg::Options { fontdb: self.fontdb.clone(), ..Default::default() };
        let tree = usvg::Tree::from_str(&combined, &opt).map_err(|e| format!("Parse: {}", e))?;
        let w = self.svg_width as u32;
        let h = self.svg_height as u32;
        let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("Failed to create pixmap")?;
        pixmap.fill(tiny_skia::Color::WHITE);
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
        pixmap.save_png(path).map_err(|e| format!("PNG: {}", e))?;
        Ok(())
    }

    pub fn select_by_id(&mut self, id: &str) {
        self.selected_element = Some(id.to_string());
        self.selected_bbox = self.element_bboxes.iter()
            .find(|(eid, _)| eid == id)
            .map(|(_, b)| *b);
        if self.tool == EditTool::Node {
            self.refresh_node_data();
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(ref id) = self.selected_element.clone() {
            let ns = svg_edit::delete_element(&self.svg_content, id);
            self.commit(ns);
            self.selected_element = None;
            self.selected_bbox = None;
            self.clear_node_data();
        }
    }

    // ─── Resize ─────────────────────────────────────────

    fn start_resize(&mut self, handle: u8, bbox: Rect) {
        self.resize_orig_bbox = bbox;
        self.resize_anchor = opposite_point(bbox, handle);
        self.resize_preview_bbox = Some(bbox);
        self.drag_mode = DragMode::Resize(handle);

        if let Some(ref id) = self.selected_element {
            let transform_str = svg_edit::get_attribute(&self.svg_content, id, "transform")
                .unwrap_or_default();
            self.resize_orig_matrix = svg_edit::parse_transform(&transform_str);
        }
    }

    fn update_resize(&mut self, handle: u8, svg_pos: Pos2) {
        let anchor = self.resize_anchor;
        let orig = self.resize_orig_bbox;

        let new_bbox = match handle {
            // Corners
            0 => Rect::from_two_pos(anchor, svg_pos), // TL dragged, anchor BR
            2 => Rect::from_two_pos(anchor, svg_pos), // TR dragged, anchor BL
            5 => Rect::from_two_pos(anchor, svg_pos), // BL dragged, anchor TR
            7 => Rect::from_two_pos(anchor, svg_pos), // BR dragged, anchor TL
            // Edges - constrain one axis
            1 => {
                // Top center: only Y changes
                let top = svg_pos.y.min(anchor.y);
                let bottom = svg_pos.y.max(anchor.y);
                Rect::from_min_max(
                    Pos2::new(orig.min.x, top),
                    Pos2::new(orig.max.x, bottom),
                )
            }
            6 => {
                // Bottom center: only Y changes
                let top = svg_pos.y.min(anchor.y);
                let bottom = svg_pos.y.max(anchor.y);
                Rect::from_min_max(
                    Pos2::new(orig.min.x, top),
                    Pos2::new(orig.max.x, bottom),
                )
            }
            3 => {
                // Left center: only X changes
                let left = svg_pos.x.min(anchor.x);
                let right = svg_pos.x.max(anchor.x);
                Rect::from_min_max(
                    Pos2::new(left, orig.min.y),
                    Pos2::new(right, orig.max.y),
                )
            }
            4 => {
                // Right center: only X changes
                let left = svg_pos.x.min(anchor.x);
                let right = svg_pos.x.max(anchor.x);
                Rect::from_min_max(
                    Pos2::new(left, orig.min.y),
                    Pos2::new(right, orig.max.y),
                )
            }
            _ => return,
        };

        // Enforce minimum size
        if new_bbox.width() > 1.0 && new_bbox.height() > 1.0 {
            self.resize_preview_bbox = Some(new_bbox);
        }
    }

    fn commit_resize(&mut self) {
        let new_bbox = match self.resize_preview_bbox {
            Some(b) => b,
            None => return,
        };
        let orig = self.resize_orig_bbox;
        let ow = orig.width();
        let oh = orig.height();
        if ow < 0.001 || oh < 0.001 {
            return;
        }

        let sx = new_bbox.width() / ow;
        let sy = new_bbox.height() / oh;

        // Scale around the anchor point in SVG space
        let ax = self.resize_anchor.x;
        let ay = self.resize_anchor.y;
        let scale_matrix = svg_edit::scale_around(sx, sy, ax, ay);
        let new_matrix = svg_edit::compose(&scale_matrix, &self.resize_orig_matrix);
        let transform_str = svg_edit::matrix_to_string(&new_matrix);

        if let Some(ref id) = self.selected_element.clone() {
            let ns = svg_edit::set_transform(&self.svg_content, id, &transform_str);
            self.commit(ns);
        }
    }

    // ─── Rotate ─────────────────────────────────────────

    fn start_rotate(&mut self, bbox: Rect, svg_pos: Pos2) {
        self.rotate_center = bbox.center();
        self.rotate_start_angle = (svg_pos.y - self.rotate_center.y)
            .atan2(svg_pos.x - self.rotate_center.x);
        self.rotate_delta = 0.0;
        self.drag_mode = DragMode::Rotate;

        if let Some(ref id) = self.selected_element {
            let transform_str = svg_edit::get_attribute(&self.svg_content, id, "transform")
                .unwrap_or_default();
            self.rotate_orig_matrix = svg_edit::parse_transform(&transform_str);
        }
    }

    fn update_rotate(&mut self, svg_pos: Pos2) {
        let current_angle = (svg_pos.y - self.rotate_center.y)
            .atan2(svg_pos.x - self.rotate_center.x);
        self.rotate_delta = current_angle - self.rotate_start_angle;
    }

    fn commit_rotate(&mut self) {
        if self.rotate_delta.abs() < 0.001 {
            return;
        }

        let cx = self.rotate_center.x;
        let cy = self.rotate_center.y;
        let rot_matrix = svg_edit::rotate_around(self.rotate_delta, cx, cy);
        let new_matrix = svg_edit::compose(&rot_matrix, &self.rotate_orig_matrix);
        let transform_str = svg_edit::matrix_to_string(&new_matrix);

        if let Some(ref id) = self.selected_element.clone() {
            let ns = svg_edit::set_transform(&self.svg_content, id, &transform_str);
            self.commit(ns);
        }
    }

    fn draw_rotated_selection(&self, painter: &egui::Painter, cr: Rect, bbox: Rect, angle: f32) {
        let center = bbox.center();
        let corners = [bbox.left_top(), bbox.right_top(), bbox.right_bottom(), bbox.left_bottom()];
        let cos = angle.cos();
        let sin = angle.sin();

        let rotated: Vec<Pos2> = corners
            .iter()
            .map(|&p| {
                let dx = p.x - center.x;
                let dy = p.y - center.y;
                let rx = center.x + dx * cos - dy * sin;
                let ry = center.y + dx * sin + dy * cos;
                self.s2s(Pos2::new(rx, ry), cr.min)
            })
            .collect();

        for i in 0..4 {
            painter.line_segment(
                [rotated[i], rotated[(i + 1) % 4]],
                Stroke::new(1.5, ACCENT),
            );
        }

        // Draw rotate handle at rotated top-center
        let top_mid_x = (corners[0].x + corners[1].x) / 2.0;
        let top_mid_y = (corners[0].y + corners[1].y) / 2.0;
        let dx = top_mid_x - center.x;
        let dy = top_mid_y - center.y - ROTATE_DIST / self.zoom;
        let rp = self.s2s(
            Pos2::new(center.x + dx * cos - dy * sin, center.y + dx * sin + dy * cos),
            cr.min,
        );
        painter.circle_filled(rp, 5.0, ACCENT);
    }

    // ─── Node editing drawing ───────────────────────────

    fn draw_path_outline(&self, painter: &egui::Painter, cr: Rect) {
        let m = &self.node_element_matrix;
        let mut pen = Pos2::ZERO;

        for cmd in &self.node_cmds {
            match cmd {
                PathCmd::Move(x, y) => {
                    pen = self.transform_to_screen(*x, *y, m, cr);
                }
                PathCmd::Line(x, y) => {
                    let to = self.transform_to_screen(*x, *y, m, cr);
                    painter.line_segment([pen, to], Stroke::new(1.0, ACCENT_DIM));
                    pen = to;
                }
                PathCmd::Cubic(x1, y1, x2, y2, x, y) => {
                    let cp1 = self.transform_to_screen(*x1, *y1, m, cr);
                    let cp2 = self.transform_to_screen(*x2, *y2, m, cr);
                    let end = self.transform_to_screen(*x, *y, m, cr);
                    let points = tessellate_cubic(pen, cp1, cp2, end, 24);
                    for w in points.windows(2) {
                        painter.line_segment([w[0], w[1]], Stroke::new(1.0, ACCENT_DIM));
                    }
                    pen = end;
                }
                PathCmd::Quad(x1, y1, x, y) => {
                    let cp = self.transform_to_screen(*x1, *y1, m, cr);
                    let end = self.transform_to_screen(*x, *y, m, cr);
                    let points = tessellate_quad(pen, cp, end, 16);
                    for w in points.windows(2) {
                        painter.line_segment([w[0], w[1]], Stroke::new(1.0, ACCENT_DIM));
                    }
                    pen = end;
                }
                PathCmd::Arc(_, _, _, _, _, x, y) => {
                    let to = self.transform_to_screen(*x, *y, m, cr);
                    // Approximate arc as straight line for outline (close enough for editing)
                    painter.line_segment([pen, to], Stroke::new(1.0, ACCENT_DIM));
                    pen = to;
                }
                PathCmd::Close => {
                    // Close path handled visually by first M point
                }
            }
        }
    }

    fn draw_node_points(&self, painter: &egui::Painter, cr: Rect, hover: Option<Pos2>) {
        let m = &self.node_element_matrix;

        // Draw control handle lines first (behind points)
        let mut prev_anchor_screen = Pos2::ZERO;
        for pt in &self.node_points {
            let screen = self.transform_to_screen(pt.pos.x, pt.pos.y, m, cr);
            match pt.kind {
                PointKind::Anchor => {
                    prev_anchor_screen = screen;
                }
                PointKind::ControlOut => {
                    painter.line_segment([prev_anchor_screen, screen], Stroke::new(1.0, HANDLE_LINE));
                }
                PointKind::ControlIn => {
                    // Find the next anchor
                    let next_anchor = self.find_next_anchor(pt.cmd_idx, pt.field);
                    if let Some(anchor_pos) = next_anchor {
                        let anchor_screen = self.transform_to_screen(anchor_pos.x, anchor_pos.y, m, cr);
                        painter.line_segment([screen, anchor_screen], Stroke::new(1.0, HANDLE_LINE));
                    }
                }
            }
        }

        // Draw points
        for (i, pt) in self.node_points.iter().enumerate() {
            let screen = self.transform_to_screen(pt.pos.x, pt.pos.y, m, cr);
            let hovered = hover.map_or(false, |h| h.distance(screen) <= NODE_HIT);
            let dragging = matches!(&self.drag_mode, DragMode::NodeDrag(idx) if *idx == i);

            match pt.kind {
                PointKind::Anchor => {
                    let size = if hovered || dragging { NODE_SIZE + 2.0 } else { NODE_SIZE };
                    let fill = if dragging {
                        ACCENT
                    } else if hovered {
                        Color32::from_rgb(150, 220, 255)
                    } else {
                        Color32::WHITE
                    };
                    painter.rect_filled(
                        Rect::from_center_size(screen, Vec2::splat(size)),
                        1.0,
                        fill,
                    );
                    painter.rect_stroke(
                        Rect::from_center_size(screen, Vec2::splat(size)),
                        1.0,
                        Stroke::new(1.5, ACCENT),
                    );
                }
                PointKind::ControlIn | PointKind::ControlOut => {
                    let radius = if hovered || dragging { 4.0 } else { 3.0 };
                    let fill = if dragging {
                        CTRL_COLOR
                    } else if hovered {
                        Color32::from_rgb(255, 200, 100)
                    } else {
                        CTRL_COLOR
                    };
                    painter.circle_filled(screen, radius, fill);
                    painter.circle_stroke(screen, radius, Stroke::new(1.0, Color32::WHITE));
                }
            }
        }
    }

    fn find_next_anchor(&self, cmd_idx: usize, field: usize) -> Option<Pos2> {
        // For ControlIn (field=1 in Cubic), the anchor is at field=2 of the same command
        if let Some(cmd) = self.node_cmds.get(cmd_idx) {
            match cmd {
                PathCmd::Cubic(_, _, _, _, x, y) if field == 1 => Some(Pos2::new(*x, *y)),
                _ => None,
            }
        } else {
            None
        }
    }

    fn transform_to_screen(&self, x: f32, y: f32, m: &svg_edit::Matrix, cr: Rect) -> Pos2 {
        let (gx, gy) = svg_edit::transform_point(m, x, y);
        self.s2s(Pos2::new(gx, gy), cr.min)
    }

    fn hit_test_node(&self, screen_pos: Pos2, cr: Rect) -> Option<usize> {
        let m = &self.node_element_matrix;
        // Check anchors first (priority), then controls
        let mut best: Option<(usize, f32)> = None;
        for (i, pt) in self.node_points.iter().enumerate() {
            let screen = self.transform_to_screen(pt.pos.x, pt.pos.y, m, cr);
            let dist = screen_pos.distance(screen);
            if dist <= NODE_HIT {
                let priority = if pt.kind == PointKind::Anchor { 0.0 } else { 1.0 };
                let key = priority * 1000.0 + dist;
                if best.map_or(true, |(_, bk)| key < bk) {
                    best = Some((i, key));
                }
            }
        }
        best.map(|(i, _)| i)
    }

    fn update_node_drag(&mut self, idx: usize, svg_pos: Pos2) {
        if idx >= self.node_points.len() {
            return;
        }

        // Convert SVG-space position to element-local coordinates
        let local_pos = if let Some(ref inv) = self.node_element_inv_matrix {
            let (lx, ly) = svg_edit::transform_point(inv, svg_pos.x, svg_pos.y);
            Pos2::new(lx, ly)
        } else {
            svg_pos
        };

        match self.node_element_tag.as_str() {
            "path" => {
                let pt = &self.node_points[idx];
                path_data::update_point(&mut self.node_cmds, pt.cmd_idx, pt.field, local_pos.x, local_pos.y);
                self.node_points = path_data::extract_points(&self.node_cmds);
            }
            _ => {
                // For shapes: directly update the point position and rebuild outline cmds
                self.node_points[idx].pos = local_pos;
                // Rebuild visual outline from points
                self.rebuild_shape_outline();
            }
        }
        self.node_dirty = true;
    }

    fn rebuild_shape_outline(&mut self) {
        match self.node_element_tag.as_str() {
            "rect" => {
                if self.node_points.len() >= 4 {
                    let pts: Vec<Pos2> = self.node_points.iter().map(|p| p.pos).collect();
                    let min_x = pts.iter().map(|p| p.x).fold(f32::MAX, f32::min);
                    let min_y = pts.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                    let max_x = pts.iter().map(|p| p.x).fold(f32::MIN, f32::max);
                    let max_y = pts.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                    let w = max_x - min_x;
                    let h = max_y - min_y;
                    // Preserve rx/ry from shape attrs
                    let rx = self.node_shape_attrs.vals.iter()
                        .find(|(n, _)| n == "rx").map(|(_, v)| *v).unwrap_or(0.0).min(w / 2.0);
                    let ry = self.node_shape_attrs.vals.iter()
                        .find(|(n, _)| n == "ry").map(|(_, v)| *v).unwrap_or(rx).min(h / 2.0);
                    self.node_cmds = build_rounded_rect_cmds(min_x, min_y, w, h, rx, ry);
                }
            }
            "circle" | "ellipse" => {
                if self.node_points.len() >= 4 {
                    let pts: Vec<Pos2> = self.node_points.iter().map(|p| p.pos).collect();
                    let cx = (pts[1].x + pts[3].x) / 2.0;
                    let cy = (pts[0].y + pts[2].y) / 2.0;
                    let rx = (pts[1].x - pts[3].x).abs() / 2.0;
                    let ry = (pts[2].y - pts[0].y).abs() / 2.0;
                    self.node_cmds = build_ellipse_cmds(cx, cy, rx, ry);
                }
            }
            _ => {
                // Polygon, polyline, line: straight line outline
                self.node_cmds.clear();
                for (i, pt) in self.node_points.iter().enumerate() {
                    if i == 0 {
                        self.node_cmds.push(PathCmd::Move(pt.pos.x, pt.pos.y));
                    } else {
                        self.node_cmds.push(PathCmd::Line(pt.pos.x, pt.pos.y));
                    }
                }
                if self.node_element_tag == "polygon" {
                    self.node_cmds.push(PathCmd::Close);
                }
            }
        }
    }

    /// Apply node changes directly to the SVG and re-render every frame
    /// during drag for real-time visual feedback.
    fn apply_node_changes_live(&mut self) {
        if !self.node_dirty {
            return;
        }

        let id = match self.selected_element.clone() {
            Some(id) => id,
            None => return,
        };

        let ns = match self.build_node_svg(&id) {
            Some(s) => s,
            None => return,
        };

        // Update SVG and texture without pushing undo (already pushed on drag start)
        self.svg_content = ns;
        self.texture_dirty = true;
        self.svg_modified = true;
    }

    /// Build modified SVG string with current node point positions applied.
    fn build_node_svg(&self, id: &str) -> Option<String> {
        let pts: Vec<Pos2> = self.node_points.iter().map(|p| p.pos).collect();
        match self.node_element_tag.as_str() {
            "path" => {
                let new_d = path_data::build(&self.node_cmds);
                Some(svg_edit::set_attribute(&self.svg_content, id, "d", &new_d))
            }
            "rect" if pts.len() >= 4 => {
                let min_x = pts.iter().map(|p| p.x).fold(f32::MAX, f32::min);
                let min_y = pts.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                let max_x = pts.iter().map(|p| p.x).fold(f32::MIN, f32::max);
                let max_y = pts.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                let mut s = self.svg_content.clone();
                s = svg_edit::set_attribute(&s, id, "x", &format!("{:.1}", min_x));
                s = svg_edit::set_attribute(&s, id, "y", &format!("{:.1}", min_y));
                s = svg_edit::set_attribute(&s, id, "width", &format!("{:.1}", max_x - min_x));
                s = svg_edit::set_attribute(&s, id, "height", &format!("{:.1}", max_y - min_y));
                Some(s)
            }
            "circle" if pts.len() >= 4 => {
                let cx = (pts[1].x + pts[3].x) / 2.0;
                let cy = (pts[0].y + pts[2].y) / 2.0;
                let rx = (pts[1].x - pts[3].x).abs() / 2.0;
                let ry = (pts[2].y - pts[0].y).abs() / 2.0;
                let r = (rx + ry) / 2.0;
                let mut s = self.svg_content.clone();
                s = svg_edit::set_attribute(&s, id, "cx", &format!("{:.1}", cx));
                s = svg_edit::set_attribute(&s, id, "cy", &format!("{:.1}", cy));
                s = svg_edit::set_attribute(&s, id, "r", &format!("{:.1}", r));
                Some(s)
            }
            "ellipse" if pts.len() >= 4 => {
                let cx = (pts[1].x + pts[3].x) / 2.0;
                let cy = (pts[0].y + pts[2].y) / 2.0;
                let rx = (pts[1].x - pts[3].x).abs() / 2.0;
                let ry = (pts[2].y - pts[0].y).abs() / 2.0;
                let mut s = self.svg_content.clone();
                s = svg_edit::set_attribute(&s, id, "cx", &format!("{:.1}", cx));
                s = svg_edit::set_attribute(&s, id, "cy", &format!("{:.1}", cy));
                s = svg_edit::set_attribute(&s, id, "rx", &format!("{:.1}", rx));
                s = svg_edit::set_attribute(&s, id, "ry", &format!("{:.1}", ry));
                Some(s)
            }
            "line" if pts.len() >= 2 => {
                let mut s = self.svg_content.clone();
                s = svg_edit::set_attribute(&s, id, "x1", &format!("{:.1}", pts[0].x));
                s = svg_edit::set_attribute(&s, id, "y1", &format!("{:.1}", pts[0].y));
                s = svg_edit::set_attribute(&s, id, "x2", &format!("{:.1}", pts[1].x));
                s = svg_edit::set_attribute(&s, id, "y2", &format!("{:.1}", pts[1].y));
                Some(s)
            }
            "polygon" | "polyline" => {
                let pts_str: String = self.node_points.iter()
                    .map(|p| format!("{:.1},{:.1}", p.pos.x, p.pos.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                Some(svg_edit::set_attribute(&self.svg_content, id, "points", &pts_str))
            }
            _ => None,
        }
    }

    // ─── ANNOTATE TOOL ──────────────────────────────────

    fn show_annotate_tool(
        &mut self,
        response: &egui::Response,
        painter: &egui::Painter,
        cr: Rect,
        hover: Option<Pos2>,
        ctx: &egui::Context,
    ) {
        const ANNO_COLOR: Color32 = Color32::from_rgb(255, 80, 60);

        // Handle text input mode
        if self.annotation_text_editing {
            // Collect relevant events first (we'll consume them after processing)
            let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
            let mut finish_text = false;
            let mut cancel_text = false;

            for event in &events {
                match event {
                    egui::Event::Text(t) => {
                        self.annotation_text_buffer.push_str(t);
                    }
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => {
                        finish_text = true;
                    }
                    egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => {
                        cancel_text = true;
                    }
                    egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                        self.annotation_text_buffer.pop();
                    }
                    _ => {}
                }
            }

            // Consume the events we processed to prevent them leaking to other widgets
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(e,
                        egui::Event::Text(_) |
                        egui::Event::Key { key: egui::Key::Enter | egui::Key::Escape | egui::Key::Backspace, pressed: true, .. }
                    )
                });
            });

            // Handle finish/cancel after event processing
            if finish_text {
                if let Some(start) = self.annotation_drag_start.take() {
                    if !self.annotation_text_buffer.is_empty() {
                        self.annotations.push(Annotation::Text {
                            pos: start,
                            text: self.annotation_text_buffer.clone(),
                        });
                    }
                }
                self.annotation_text_editing = false;
                self.annotation_text_buffer.clear();
            } else if cancel_text {
                self.annotation_text_editing = false;
                self.annotation_text_buffer.clear();
                self.annotation_drag_start = None;
            }

            // Draw text cursor
            if let Some(pos) = self.annotation_drag_start {
                let screen_pos = self.s2s(pos, cr.min);
                let display = format!("{}|", self.annotation_text_buffer);
                painter.text(screen_pos, egui::Align2::LEFT_TOP, &display,
                    egui::FontId::proportional(14.0), ANNO_COLOR);
            }
            return;
        }

        match self.annotation_tool {
            AnnotationTool::Circle | AnnotationTool::Arrow => {
                // Drag to create circle/arrow
                if response.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(pos) = hover {
                        self.annotation_drag_start = Some(self.s2v(pos, cr.min));
                    }
                }

                // Preview while dragging
                if response.dragged_by(egui::PointerButton::Primary) {
                    if let (Some(start), Some(pos)) = (self.annotation_drag_start, hover) {
                        let end = self.s2v(pos, cr.min);
                        let s_start = self.s2s(start, cr.min);
                        let s_end = self.s2s(end, cr.min);

                        match self.annotation_tool {
                            AnnotationTool::Circle => {
                                let center = Pos2::new((s_start.x + s_end.x) / 2.0, (s_start.y + s_end.y) / 2.0);
                                let rx = (s_end.x - s_start.x).abs() / 2.0;
                                let ry = (s_end.y - s_start.y).abs() / 2.0;
                                painter.circle_stroke(center, rx.max(ry), Stroke::new(2.5, ANNO_COLOR));
                            }
                            AnnotationTool::Arrow => {
                                draw_arrow(painter, s_start, s_end, ANNO_COLOR, 2.5);
                            }
                            _ => {}
                        }
                    }
                }

                // Commit on release
                if response.drag_stopped() {
                    if let (Some(start), Some(pos)) = (self.annotation_drag_start.take(), hover) {
                        let end = self.s2v(pos, cr.min);
                        match self.annotation_tool {
                            AnnotationTool::Circle => {
                                let cx = (start.x + end.x) / 2.0;
                                let cy = (start.y + end.y) / 2.0;
                                let rx = (end.x - start.x).abs() / 2.0;
                                let ry = (end.y - start.y).abs() / 2.0;
                                if rx > 2.0 || ry > 2.0 {
                                    self.annotations.push(Annotation::Circle {
                                        center: Pos2::new(cx, cy),
                                        radius_x: rx,
                                        radius_y: ry,
                                    });
                                }
                            }
                            AnnotationTool::Arrow => {
                                if start.distance(end) > 5.0 {
                                    self.annotations.push(Annotation::Arrow { start, end });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            AnnotationTool::Text => {
                // Click to place text, then type
                if response.clicked() {
                    if let Some(pos) = hover {
                        let svp = self.s2v(pos, cr.min);
                        self.annotation_drag_start = Some(svp);
                        self.annotation_text_editing = true;
                        self.annotation_text_buffer.clear();
                    }
                }
            }
        }
    }

    /// Draw all annotations (called in every tool mode).
    fn draw_annotations(&self, painter: &egui::Painter, cr: Rect, _hover: Option<Pos2>) {
        const ANNO_COLOR: Color32 = Color32::from_rgb(255, 80, 60);
        const ANNO_STROKE: f32 = 2.5;

        for ann in &self.annotations {
            match ann {
                Annotation::Circle { center, radius_x, radius_y } => {
                    let sc = self.s2s(*center, cr.min);
                    let srx = radius_x * self.zoom;
                    let sry = radius_y * self.zoom;
                    // Draw as ellipse using dashed circle (approximate with circle for now)
                    painter.circle_stroke(sc, srx.max(sry), Stroke::new(ANNO_STROKE, ANNO_COLOR));
                    // If significantly elliptical, draw a second inner ellipse hint
                    if (srx - sry).abs() > 5.0 {
                        let r = Rect::from_center_size(sc, Vec2::new(srx * 2.0, sry * 2.0));
                        painter.rect_stroke(r, srx.min(sry), Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 80, 60, 80)));
                    }
                }
                Annotation::Arrow { start, end } => {
                    let s_start = self.s2s(*start, cr.min);
                    let s_end = self.s2s(*end, cr.min);
                    draw_arrow(painter, s_start, s_end, ANNO_COLOR, ANNO_STROKE);
                }
                Annotation::Text { pos, text } => {
                    let sp = self.s2s(*pos, cr.min);
                    // Background for readability
                    let galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(14.0), ANNO_COLOR);
                    let text_rect = egui::Align2::LEFT_TOP.anchor_size(sp, galley.size());
                    painter.rect_filled(text_rect.expand(3.0), 2.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180));
                    painter.galley(sp, galley, ANNO_COLOR);
                }
            }
        }
    }
}

/// Draw an arrow line with an arrowhead.
fn draw_arrow(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32, width: f32) {
    painter.line_segment([start, end], Stroke::new(width, color));

    // Arrowhead
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 { return; }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = 12.0;
    let head_width = 6.0;
    let base = Pos2::new(end.x - ux * head_len, end.y - uy * head_len);
    let left = Pos2::new(base.x - uy * head_width, base.y + ux * head_width);
    let right = Pos2::new(base.x + uy * head_width, base.y - ux * head_width);
    painter.add(egui::Shape::convex_polygon(vec![end, left, right], color, Stroke::NONE));
}

// ─── Free functions ─────────────────────────────────────

fn resize_handles(r: Rect) -> [Pos2; 8] {
    let cx = r.center().x;
    let cy = r.center().y;
    [
        r.left_top(),
        Pos2::new(cx, r.min.y),
        r.right_top(),
        Pos2::new(r.min.x, cy),
        Pos2::new(r.max.x, cy),
        r.left_bottom(),
        Pos2::new(cx, r.max.y),
        r.right_bottom(),
    ]
}

/// Get the point opposite to handle `idx` on the bounding box.
fn opposite_point(bbox: Rect, handle: u8) -> Pos2 {
    let cx = bbox.center().x;
    let cy = bbox.center().y;
    match handle {
        0 => bbox.right_bottom(), // TL → BR
        1 => Pos2::new(cx, bbox.max.y), // TC → BC
        2 => bbox.left_bottom(),  // TR → BL
        3 => Pos2::new(bbox.max.x, cy), // ML → MR
        4 => Pos2::new(bbox.min.x, cy), // MR → ML
        5 => bbox.right_top(),    // BL → TR
        6 => Pos2::new(cx, bbox.min.y), // BC → TC
        7 => bbox.left_top(),     // BR → TL
        _ => bbox.center(),
    }
}

fn draw_dot_grid(p: &egui::Painter, r: Rect) {
    let sp = 20.0;
    let c = Color32::from_rgba_unmultiplied(255, 255, 255, 18);
    let sx = (r.min.x / sp).floor() as i32;
    let ex = (r.max.x / sp).ceil() as i32;
    let sy = (r.min.y / sp).floor() as i32;
    let ey = (r.max.y / sp).ceil() as i32;
    for ix in sx..=ex {
        for iy in sy..=ey {
            p.circle_filled(Pos2::new(ix as f32 * sp, iy as f32 * sp), 0.8, c);
        }
    }
}

fn apply_transform(r: Rect, t: &usvg::Transform) -> Rect {
    let min = Pos2::new(
        t.sx * r.min.x + t.kx * r.min.y + t.tx,
        t.ky * r.min.x + t.sy * r.min.y + t.ty,
    );
    let max = Pos2::new(
        t.sx * r.max.x + t.kx * r.max.y + t.tx,
        t.ky * r.max.x + t.sy * r.max.y + t.ty,
    );
    Rect::from_min_max(
        Pos2::new(min.x.min(max.x), min.y.min(max.y)),
        Pos2::new(min.x.max(max.x), min.y.max(max.y)),
    )
}

fn usvg_rect(b: tiny_skia::Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(b.left(), b.top()),
        Pos2::new(b.right(), b.bottom()),
    )
}

fn collect_bboxes_ordered(
    out: &mut Vec<(String, Rect)>,
    group: &usvg::Group,
    parent_transform: usvg::Transform,
) {
    for child in group.children().iter() {
        match child {
            usvg::Node::Group(g) => {
                let combined = parent_transform.pre_concat(g.transform());
                let id = g.id().to_string();
                if !id.is_empty() {
                    let b = g.bounding_box();
                    out.push((id, apply_transform(usvg_rect(b), &combined)));
                }
                collect_bboxes_ordered(out, g, combined);
            }
            usvg::Node::Path(p) => {
                let id = p.id().to_string();
                if !id.is_empty() {
                    let b = p.bounding_box();
                    out.push((id, apply_transform(usvg_rect(b), &parent_transform)));
                }
            }
            usvg::Node::Image(img) => {
                let id = img.id().to_string();
                if !id.is_empty() {
                    let b = img.bounding_box();
                    out.push((id, apply_transform(usvg_rect(b), &parent_transform)));
                }
            }
            usvg::Node::Text(t) => {
                let id = t.id().to_string();
                if !id.is_empty() {
                    let b = t.bounding_box();
                    out.push((id, apply_transform(usvg_rect(b), &parent_transform)));
                }
            }
        }
    }
}

fn tessellate_cubic(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, segments: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let mt = 1.0 - t;
        pts.push(Pos2::new(
            mt * mt * mt * p0.x + 3.0 * mt * mt * t * p1.x + 3.0 * mt * t * t * p2.x + t * t * t * p3.x,
            mt * mt * mt * p0.y + 3.0 * mt * mt * t * p1.y + 3.0 * mt * t * t * p2.y + t * t * t * p3.y,
        ));
    }
    pts
}

fn tessellate_quad(p0: Pos2, p1: Pos2, p2: Pos2, segments: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let mt = 1.0 - t;
        pts.push(Pos2::new(
            mt * mt * p0.x + 2.0 * mt * t * p1.x + t * t * p2.x,
            mt * mt * p0.y + 2.0 * mt * t * p1.y + t * t * p2.y,
        ));
    }
    pts
}

/// Build cubic bezier path commands for an ellipse (4-arc approximation).
/// The magic number 0.5522847498 (kappa) gives a near-perfect circular arc.
fn build_ellipse_cmds(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<PathCmd> {
    let k: f32 = 0.5522847498;
    let kx = rx * k;
    let ky = ry * k;
    vec![
        PathCmd::Move(cx, cy - ry),
        // Top to Right
        PathCmd::Cubic(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
        // Right to Bottom
        PathCmd::Cubic(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
        // Bottom to Left
        PathCmd::Cubic(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
        // Left to Top
        PathCmd::Cubic(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
        PathCmd::Close,
    ]
}

/// Build path commands for a rounded rect (rx/ry corner radii).
fn build_rounded_rect_cmds(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> Vec<PathCmd> {
    if rx < 0.1 && ry < 0.1 {
        // Sharp corners
        return vec![
            PathCmd::Move(x, y),
            PathCmd::Line(x + w, y),
            PathCmd::Line(x + w, y + h),
            PathCmd::Line(x, y + h),
            PathCmd::Close,
        ];
    }
    let k: f32 = 0.5522847498;
    let kx = rx * k;
    let ky = ry * k;
    vec![
        PathCmd::Move(x + rx, y),
        // Top edge
        PathCmd::Line(x + w - rx, y),
        // Top-right corner
        PathCmd::Cubic(x + w - rx + kx, y, x + w, y + ry - ky, x + w, y + ry),
        // Right edge
        PathCmd::Line(x + w, y + h - ry),
        // Bottom-right corner
        PathCmd::Cubic(x + w, y + h - ry + ky, x + w - rx + kx, y + h, x + w - rx, y + h),
        // Bottom edge
        PathCmd::Line(x + rx, y + h),
        // Bottom-left corner
        PathCmd::Cubic(x + rx - kx, y + h, x, y + h - ry + ky, x, y + h - ry),
        // Left edge
        PathCmd::Line(x, y + ry),
        // Top-left corner
        PathCmd::Cubic(x, y + ry - ky, x + rx - kx, y, x + rx, y),
        PathCmd::Close,
    ]
}
