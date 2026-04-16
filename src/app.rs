use std::path::PathBuf;
use std::collections::VecDeque;

use crate::canvas::{CanvasState, EditTool};
use crate::layers::LayerPanel;
use crate::project;
use crate::svg_edit;
use crate::svg_ops;
use crate::watcher::FileWatcher;

const MAX_RECENT: usize = 10;

pub struct ForgeApp {
    project_dir: PathBuf,
    active_file: Option<PathBuf>,
    canvas: CanvasState,
    layers: LayerPanel,
    watcher: Option<FileWatcher>,
    mode: &'static str,
    status_msg: String,
    needs_fit: bool,
    unsaved: bool,
    recent_files: VecDeque<PathBuf>,
    show_layers: bool,
}

impl ForgeApp {
    pub fn new(project_dir: PathBuf) -> Self {
        let mut app = Self {
            project_dir: project_dir.clone(),
            active_file: None,
            canvas: CanvasState::new(),
            layers: LayerPanel::new(),
            watcher: None,
            mode: "static",
            status_msg: String::new(),
            needs_fit: true,
            unsaved: false,
            recent_files: VecDeque::new(),
            show_layers: true,
        };

        match FileWatcher::new(&project_dir) {
            Ok(w) => app.watcher = Some(w),
            Err(e) => app.status_msg = format!("Watcher: {}", e),
        }

        if let Some(path) = project::pick_active_file(&project_dir) {
            app.load_file(&path);
        }

        // Load recent files list from disk
        app.load_recent_files();
        app
    }

    fn load_file(&mut self, path: &PathBuf) {
        match project::read_svg(path) {
            Ok(content) => {
                self.canvas.load_svg(content);
                self.mode = if self.canvas.is_animated { "animated" } else { "static" };
                self.layers.refresh(&self.canvas.svg_content);
                self.active_file = Some(path.clone());
                self.needs_fit = true;
                self.unsaved = false;
                self.add_recent(path.clone());
                let anim_info = if self.canvas.is_animated {
                    format!(" ({} anims, {:.1}s)", self.canvas.animations.len(), self.canvas.anim_duration)
                } else { String::new() };
                self.status_msg = format!("Loaded: {}{}", path.file_name().unwrap_or_default().to_string_lossy(), anim_info);

                // Update watcher to watch the file's directory
                let dir = path.parent().unwrap_or(path).to_path_buf();
                if dir != self.project_dir {
                    self.project_dir = dir.clone();
                    self.watcher = FileWatcher::new(&dir).ok();
                }
            }
            Err(e) => self.status_msg = format!("Error: {}", e),
        }
    }

    fn save(&mut self) {
        if let Some(ref path) = self.active_file {
            match project::write_svg(path, &self.canvas.svg_content) {
                Ok(()) => { self.unsaved = false; self.status_msg = "Saved".to_string(); }
                Err(e) => self.status_msg = format!("Save error: {}", e),
            }
        } else {
            self.save_as();
        }
    }

    fn save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save SVG As")
            .add_filter("SVG", &["svg"])
            .save_file()
        {
            self.active_file = Some(path.clone());
            self.save();
            self.add_recent(path);
        }
    }

    fn new_static(&mut self) {
        if self.unsaved && !self.confirm_discard() { return; }
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:forge="https://svgforge.dev/ns"
     viewBox="0 0 1920 1080" forge:mode="static" forge:version="1">
</svg>"#;
        self.canvas.load_svg(svg.to_string());
        self.mode = "static";
        self.layers.refresh(&self.canvas.svg_content);
        self.active_file = None;
        self.needs_fit = true;
        self.unsaved = false;
        self.status_msg = "New static project".into();
    }

    fn new_animated(&mut self) {
        if self.unsaved && !self.confirm_discard() { return; }
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:forge="https://svgforge.dev/ns"
     viewBox="0 0 1920 1080" forge:mode="animated" forge:version="1">
</svg>"#;
        self.canvas.load_svg(svg.to_string());
        self.mode = "animated";
        self.layers.refresh(&self.canvas.svg_content);
        self.active_file = None;
        self.needs_fit = true;
        self.unsaved = false;
        self.status_msg = "New animated project".into();
    }

    fn open_file_dialog(&mut self) {
        if self.unsaved && !self.confirm_discard() { return; }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open SVG")
            .add_filter("SVG files", &["svg"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.load_file(&path);
        }
    }

    fn import_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Import File")
            .add_filter("Images", &["svg", "png", "jpg", "jpeg", "gif", "webp", "bmp"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "svg" => {
                    // Import SVG as a layer into current project
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        self.import_svg_as_layer(&content, &path);
                    }
                }
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => {
                    self.embed_image_file(&path);
                }
                _ => self.status_msg = format!("Unsupported: {}", ext),
            }
        }
    }

    fn import_svg_as_layer(&mut self, content: &str, path: &std::path::Path) {
        // Wrap imported SVG content in a group layer
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let layer_id = format!("layer-import-{}", &uuid::Uuid::new_v4().to_string()[..8]);

        // Extract everything between <svg ...> and </svg>
        let inner = if let (Some(start), Some(end)) = (content.find('>'), content.rfind("</svg>")) {
            &content[start + 1..end]
        } else {
            content
        };

        let el = format!(
            "  <g id=\"{}\" forge:name=\"{}\" forge:visible=\"true\" forge:locked=\"false\" forge:order=\"99\">\n{}\n  </g>\n",
            layer_id, name, inner
        );

        if self.canvas.svg_content.is_empty() {
            // No project open — load the SVG directly
            self.canvas.load_svg(content.to_string());
            self.mode = if self.canvas.is_animated { "animated" } else { "static" };
        } else if let Some(pos) = self.canvas.svg_content.rfind("</svg>") {
            let mut new_svg = self.canvas.svg_content.clone();
            new_svg.insert_str(pos, &el);
            self.canvas.load_svg_with_undo(new_svg);
            self.unsaved = true;
        }
        self.layers.refresh(&self.canvas.svg_content);
        self.status_msg = format!("Imported: {}", name);
    }

    fn export_dialog(&mut self) {
        if self.canvas.svg_content.is_empty() {
            self.status_msg = "Nothing to export".into();
            return;
        }

        if let Some(path) = rfd::FileDialog::new()
            .set_title("Export")
            .add_filter("SVG", &["svg"])
            .add_filter("PNG", &["png"])
            .save_file()
        {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("svg");
            match ext {
                "png" => self.export_png(&path),
                _ => {
                    match project::write_svg(&path, &self.canvas.svg_content) {
                        Ok(()) => self.status_msg = format!("Exported: {}", path.display()),
                        Err(e) => self.status_msg = format!("Export error: {}", e),
                    }
                }
            }
        }
    }

    fn export_png(&mut self, path: &std::path::Path) {
        let opt = usvg::Options::default();
        let tree = match usvg::Tree::from_str(&self.canvas.svg_content, &opt) {
            Ok(t) => t,
            Err(e) => { self.status_msg = format!("Parse error: {}", e); return; }
        };
        let w = self.canvas.svg_width as u32;
        let h = self.canvas.svg_height as u32;
        let mut pixmap = match tiny_skia::Pixmap::new(w, h) {
            Some(p) => p,
            None => { self.status_msg = "Failed to create pixmap".into(); return; }
        };
        pixmap.fill(tiny_skia::Color::WHITE);
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
        match pixmap.save_png(path) {
            Ok(()) => self.status_msg = format!("Exported PNG: {}", path.display()),
            Err(e) => self.status_msg = format!("PNG error: {}", e),
        }
    }

    fn confirm_discard(&self) -> bool {
        // Simple confirmation via rfd
        rfd::MessageDialog::new()
            .set_title("Unsaved Changes")
            .set_description("You have unsaved changes. Discard them?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .show() == rfd::MessageDialogResult::Yes
    }

    // ─── Recent files ────────────────────────────────────

    fn add_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.push_front(path);
        while self.recent_files.len() > MAX_RECENT {
            self.recent_files.pop_back();
        }
        self.save_recent_files();
    }

    fn recent_file_path() -> PathBuf {
        // Use user config directory instead of exe directory
        if let Some(config_dir) = dirs::config_dir() {
            let app_dir = config_dir.join("svg-forge");
            let _ = std::fs::create_dir_all(&app_dir);
            return app_dir.join("recent-files.txt");
        }
        // Fallback to exe directory
        let mut p = std::env::current_exe().unwrap_or_default();
        p.pop();
        p.push("svg-forge-recent.txt");
        p
    }

    fn load_recent_files(&mut self) {
        if let Ok(content) = std::fs::read_to_string(Self::recent_file_path()) {
            for line in content.lines() {
                let p = PathBuf::from(line.trim());
                if p.exists() && self.recent_files.len() < MAX_RECENT {
                    self.recent_files.push_back(p);
                }
            }
        }
    }

    fn save_recent_files(&self) {
        let content: String = self.recent_files.iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(Self::recent_file_path(), content);
    }

    // ─── File helpers ────────────────────────────────────

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        for file in &dropped {
            if let Some(path) = &file.path {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                match ext {
                    "svg" => { self.load_file(&path.clone()); return; }
                    "png"|"jpg"|"jpeg"|"gif"|"webp"|"bmp" => { self.embed_image_file(path); return; }
                    _ => self.status_msg = format!("Unsupported: {}", ext),
                }
            }
        }
    }

    fn embed_image_file(&mut self, path: &std::path::Path) {
        let data = match std::fs::read(path) { Ok(d) => d, Err(e) => { self.status_msg = format!("Read error: {}", e); return; } };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
        let mime = match ext { "jpg"|"jpeg"=>"image/jpeg", "gif"=>"image/gif", "webp"=>"image/webp", _=>"image/png" };
        let b64 = base64_encode(&data);
        let data_url = format!("data:{};base64,{}", mime, b64);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let layer_id = format!("layer-img-{}", &uuid::Uuid::new_v4().to_string()[..8]);

        // Try to detect image dimensions, fallback to canvas-relative size
        let (w, h) = detect_image_size(&data).unwrap_or_else(|| {
            // Default to half the canvas size, centered
            ((self.canvas.svg_width * 0.5) as u32, (self.canvas.svg_height * 0.5) as u32)
        });
        // Center the image on the canvas
        let x = ((self.canvas.svg_width - w as f32) / 2.0).max(0.0) as u32;
        let y = ((self.canvas.svg_height - h as f32) / 2.0).max(0.0) as u32;

        let el = format!(
            r##"  <g id="{}" forge:name="{}" forge:visible="true" forge:locked="false" forge:order="99">
    <image href="{}" x="{}" y="{}" width="{}" height="{}"/>
  </g>
"##, layer_id, name, data_url, x, y, w, h);
        if let Some(pos) = self.canvas.svg_content.rfind("</svg>") {
            let mut new_svg = self.canvas.svg_content.clone();
            new_svg.insert_str(pos, &el);
            self.canvas.load_svg_with_undo(new_svg);
            self.layers.refresh(&self.canvas.svg_content);
            self.unsaved = true;
            self.status_msg = format!("Embedded: {}", name);
        }
    }
}

// ─── Main update loop ────────────────────────────────────

impl eframe::App for ForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // File watcher
        if let Some(ref mut w) = self.watcher {
            if let Some(p) = w.poll() {
                if self.active_file.as_ref() == Some(&p) && !self.unsaved { self.load_file(&p); }
                ctx.request_repaint();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        if self.canvas.svg_modified {
            self.canvas.svg_modified = false;
            self.unsaved = true;
            self.layers.refresh(&self.canvas.svg_content);
        }

        self.handle_dropped_files(ctx);
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let s = ctx.screen_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            p.rect_filled(s, 0.0, egui::Color32::from_rgba_unmultiplied(79, 195, 247, 30));
            p.rect_stroke(s, 0.0, egui::Stroke::new(3.0, egui::Color32::from_rgb(79, 195, 247)));
            p.text(s.center(), egui::Align2::CENTER_CENTER, "Drop file here", egui::FontId::proportional(24.0), egui::Color32::from_rgb(79, 195, 247));
        }

        // Keyboard shortcuts
        if ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.command && !i.modifiers.shift) { self.save(); }
        if ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.command && i.modifiers.shift) { self.save_as(); }
        if ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command) { self.open_file_dialog(); }
        if ctx.input(|i| i.key_pressed(egui::Key::N) && i.modifiers.command) { self.new_static(); }
        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command && !i.modifiers.shift) {
            if self.canvas.undo() { self.unsaved = true; self.layers.refresh(&self.canvas.svg_content); self.status_msg = "Undo".into(); }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command && i.modifiers.shift) {
            if self.canvas.redo() { self.unsaved = true; self.layers.refresh(&self.canvas.svg_content); self.status_msg = "Redo".into(); }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            self.canvas.delete_selected();
            self.layers.refresh(&self.canvas.svg_content);
            self.unsaved = true;
        }
        // Tool switching
        if ctx.input(|i| i.key_pressed(egui::Key::V) && !i.modifiers.command) {
            self.canvas.tool = EditTool::Select;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::A) && !i.modifiers.command) {
            self.canvas.tool = EditTool::Node;
        }

        // ─── Menu bar ───────────────────────────────────

        egui::TopBottomPanel::top("menubar").show(ctx, |ui: &mut egui::Ui| {
            egui::menu::bar(ui, |ui: &mut egui::Ui| {
                // ── File ──
                ui.menu_button("File", |ui: &mut egui::Ui| {
                    if ui.button("New Static          Ctrl+N").clicked() { ui.close_menu(); self.new_static(); }
                    if ui.button("New Animated").clicked() { ui.close_menu(); self.new_animated(); }
                    ui.separator();
                    if ui.button("Open...             Ctrl+O").clicked() { ui.close_menu(); self.open_file_dialog(); }

                    // Open Recent submenu
                    ui.menu_button("Open Recent", |ui: &mut egui::Ui| {
                        if self.recent_files.is_empty() {
                            ui.label("(none)");
                        } else {
                            let recents: Vec<PathBuf> = self.recent_files.iter().cloned().collect();
                            for path in &recents {
                                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                if ui.button(&name).on_hover_text(path.to_string_lossy().to_string()).clicked() {
                                    ui.close_menu();
                                    self.load_file(path);
                                }
                            }
                        }
                    });

                    ui.separator();
                    if ui.button("Save                Ctrl+S").clicked() { ui.close_menu(); self.save(); }
                    if ui.button("Save As...     Ctrl+Shift+S").clicked() { ui.close_menu(); self.save_as(); }
                    ui.separator();
                    if ui.button("Import...").clicked() { ui.close_menu(); self.import_file_dialog(); }
                    if ui.button("Export...").clicked() { ui.close_menu(); self.export_dialog(); }
                });

                // ── Edit ──
                ui.menu_button("Edit", |ui: &mut egui::Ui| {
                    let can_undo = !self.canvas.svg_content.is_empty();
                    if ui.add_enabled(can_undo, egui::Button::new("Undo                Ctrl+Z")).clicked() {
                        ui.close_menu();
                        if self.canvas.undo() { self.unsaved = true; self.layers.refresh(&self.canvas.svg_content); }
                    }
                    if ui.add_enabled(can_undo, egui::Button::new("Redo           Ctrl+Shift+Z")).clicked() {
                        ui.close_menu();
                        if self.canvas.redo() { self.unsaved = true; self.layers.refresh(&self.canvas.svg_content); }
                    }
                    ui.separator();
                    let has_sel = self.canvas.selected_element.is_some();
                    if ui.add_enabled(has_sel, egui::Button::new("Delete             Del")).clicked() {
                        ui.close_menu();
                        self.canvas.delete_selected();
                        self.layers.refresh(&self.canvas.svg_content);
                        self.unsaved = true;
                    }
                    ui.separator();
                    if ui.button("Select All").clicked() {
                        ui.close_menu();
                        // Select first top-level layer
                        if let Some(first) = self.layers.tree.first() {
                            if !first.id.is_empty() { self.canvas.select_by_id(&first.id); }
                        }
                    }
                    if ui.button("Deselect").clicked() {
                        ui.close_menu();
                        self.canvas.selected_element = None;
                        self.canvas.selected_bbox = None;
                        self.layers.selected_id = None;
                    }
                });

                // ── Window ──
                ui.menu_button("Window", |ui: &mut egui::Ui| {
                    if ui.checkbox(&mut self.show_layers, "Layers Panel").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Fit to View").clicked() { ui.close_menu(); self.needs_fit = true; }
                    if ui.button("Reset Zoom (100%)").clicked() {
                        ui.close_menu();
                        self.canvas.zoom = 1.0;
                    }
                });

                // ── Right side: info ──
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                    ui.small(&self.status_msg);
                    if self.unsaved { ui.colored_label(egui::Color32::from_rgb(255,167,38), "●"); }
                    let mc = if self.mode == "animated" { egui::Color32::from_rgb(255,167,38) } else { egui::Color32::from_rgb(102,187,106) };
                    ui.colored_label(mc, self.mode);
                    ui.separator();
                    ui.small(format!("{}%", (self.canvas.zoom * 100.0) as i32));
                    if ui.small_button("Fit").clicked() { self.needs_fit = true; }
                });
            });
        });

        // ─── Status bar ──────────────────────────────────
        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui: &mut egui::Ui| {
            ui.horizontal(|ui: &mut egui::Ui| {
                if let Some(ref p) = self.active_file {
                    ui.label(p.to_string_lossy().to_string());
                } else {
                    ui.label("(untitled)");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                    ui.label(format!("{} × {}", self.canvas.svg_width as i32, self.canvas.svg_height as i32));
                    if let Some(ref s) = self.canvas.selected_element { ui.separator(); ui.label(format!("Selected: {}", s)); }
                });
            });
        });

        // ─── Layers panel ────────────────────────────────
        if self.show_layers {
            egui::SidePanel::right("layers").default_width(240.0).min_width(180.0).show(ctx, |ui: &mut egui::Ui| {
                self.layers.show(ui);
            });
        }

        // Process layer actions
        if let Some((id, vis)) = self.layers.pending_toggle.take() {
            let new = svg_edit::set_forge_visible(&self.canvas.svg_content, &id, vis);
            self.canvas.load_svg_with_undo(new);
            self.layers.refresh(&self.canvas.svg_content);
            self.unsaved = true;
        }
        if let Some(id) = self.layers.pending_delete.take() {
            let new = svg_edit::delete_element(&self.canvas.svg_content, &id);
            self.canvas.load_svg_with_undo(new);
            self.layers.refresh(&self.canvas.svg_content);
            self.unsaved = true;
        }
        if let Some(id) = self.layers.pending_select.take() { self.canvas.select_by_id(&id); }
        if let Some((drag_id, target_id, dir)) = self.layers.pending_reorder.take() {
            let new = svg_edit::reorder_element(&self.canvas.svg_content, &drag_id, &target_id, dir);
            self.canvas.load_svg_with_undo(new);
            self.layers.refresh(&self.canvas.svg_content);
            self.unsaved = true;
        }

        // ─── Animation tick ──────────────────────────────
        if self.canvas.tick_animation() { ctx.request_repaint(); }
        if self.canvas.is_animated && ctx.input(|i| i.key_pressed(egui::Key::Space) && !i.modifiers.command) {
            self.canvas.play_pause();
        }

        // ─── Timeline (animated mode) ────────────────────
        if self.canvas.is_animated {
            egui::TopBottomPanel::bottom("timeline").min_height(48.0).default_height(60.0).show(ctx, |ui: &mut egui::Ui| {
                ui.horizontal(|ui: &mut egui::Ui| {
                    let icon = if self.canvas.anim_playing { "⏸" } else { "▶" };
                    if ui.button(icon).clicked() { self.canvas.play_pause(); }
                    ui.label(format!("{:.2}s / {:.2}s", self.canvas.anim_time, self.canvas.anim_duration));
                    ui.separator();
                    let mut t = self.canvas.anim_time as f32;
                    let slider = egui::Slider::new(&mut t, 0.0..=self.canvas.anim_duration as f32).show_value(false);
                    if ui.add_sized(egui::vec2(ui.available_width() - 40.0, 20.0), slider).changed() {
                        self.canvas.seek(t as f64);
                    }
                    ui.label("🔁");
                });
            });
        }

        // ─── Toolbar ─────────────────────────────────────
        egui::SidePanel::left("toolbar")
            .exact_width(36.0)
            .resizable(false)
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(50, 50, 50))
                .inner_margin(egui::Margin::symmetric(4.0, 8.0)))
            .show(ctx, |ui: &mut egui::Ui| {
                ui.vertical_centered(|ui: &mut egui::Ui| {
                    let sel = ui.add(egui::SelectableLabel::new(
                        self.canvas.tool == EditTool::Select,
                        egui::RichText::new("\u{25AD}").size(18.0),
                    ));
                    if sel.clicked() { self.canvas.tool = EditTool::Select; }
                    sel.on_hover_text("Select / Move / Resize / Rotate (V)");

                    ui.add_space(4.0);

                    let node = ui.add(egui::SelectableLabel::new(
                        self.canvas.tool == EditTool::Node,
                        egui::RichText::new("\u{25C7}").size(18.0),
                    ));
                    if node.clicked() { self.canvas.tool = EditTool::Node; }
                    node.on_hover_text("Node / Direct Selection (A)");
                });
            });

        // ─── Canvas ──────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(42, 42, 42)))
            .show(ctx, |ui: &mut egui::Ui| {
                if self.needs_fit { self.canvas.fit_to_view(ui.available_size()); self.needs_fit = false; }
                self.canvas.show(ui, ctx);
            });
    }
}

/// Detect image dimensions from file header bytes (PNG, JPEG, GIF, BMP, WebP).
fn detect_image_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 30 { return None; }
    // PNG: bytes 16-23 contain width and height as big-endian u32
    if data.starts_with(b"\x89PNG\r\n\x1a\n") && data.len() >= 24 {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((w, h));
    }
    // GIF: bytes 6-9 contain width and height as little-endian u16
    if data.starts_with(b"GIF8") && data.len() >= 10 {
        let w = u16::from_le_bytes([data[6], data[7]]) as u32;
        let h = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some((w, h));
    }
    // BMP: bytes 18-25 contain width and height as little-endian i32
    if data.starts_with(b"BM") && data.len() >= 26 {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]).unsigned_abs();
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]).unsigned_abs();
        return Some((w, h));
    }
    // JPEG: scan for SOF0/SOF2 marker
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF { i += 1; continue; }
            let marker = data[i + 1];
            if marker == 0xC0 || marker == 0xC2 {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((w, h));
            }
            // Skip to next marker; segment length includes the 2-byte length field itself
            if i + 3 >= data.len() { break; }
            let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if seg_len < 2 { break; } // Invalid segment, abort
            i += 2 + seg_len;
        }
    }
    None
}

fn base64_encode(data: &[u8]) -> String {
    const C: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut r = String::with_capacity((data.len() + 2) / 3 * 4);
    for ch in data.chunks(3) {
        let b0 = ch[0] as u32;
        let b1 = if ch.len() > 1 { ch[1] as u32 } else { 0 };
        let b2 = if ch.len() > 2 { ch[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        r.push(C[((n >> 18) & 63) as usize] as char);
        r.push(C[((n >> 12) & 63) as usize] as char);
        r.push(if ch.len() > 1 { C[((n >> 6) & 63) as usize] as char } else { '=' });
        r.push(if ch.len() > 2 { C[(n & 63) as usize] as char } else { '=' });
    }
    r
}
