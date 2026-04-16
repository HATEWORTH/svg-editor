use crate::svg_ops::{self, LayerNode};

pub struct LayerPanel {
    pub tree: Vec<LayerNode>,
    pub selected_id: Option<String>,
    pub pending_select: Option<String>,
    pub pending_toggle: Option<(String, bool)>,
    pub pending_delete: Option<String>,
    pub pending_reorder: Option<(String, String, ReorderDir)>,
    // Drag state
    dragging_id: Option<String>,
    item_rects: Vec<(String, egui::Rect)>, // collected each frame
}

#[derive(Clone, Copy, PartialEq)]
pub enum ReorderDir { Before, After, Into }

impl LayerPanel {
    pub fn new() -> Self {
        Self {
            tree: Vec::new(), selected_id: None,
            pending_select: None, pending_toggle: None,
            pending_delete: None, pending_reorder: None,
            dragging_id: None, item_rects: Vec::new(),
        }
    }

    pub fn refresh(&mut self, svg_content: &str) {
        let old_exp = collect_expanded(&self.tree);
        self.tree = svg_ops::parse_layer_tree(svg_content);
        restore_expanded(&mut self.tree, &old_exp);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.pending_select = None;
        self.pending_toggle = None;
        self.pending_delete = None;
        self.pending_reorder = None;
        self.item_rects.clear();

        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let pointer_down = ui.input(|i| i.pointer.primary_down());
        let pointer_released = ui.input(|i| i.pointer.primary_released());

        // Header
        ui.horizontal(|ui: &mut egui::Ui| {
            ui.strong("Layers");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                if ui.small_button("🗑").on_hover_text("Delete selected").clicked() {
                    if let Some(ref id) = self.selected_id {
                        if !id.is_empty() { self.pending_delete = Some(id.clone()); }
                    }
                }
                // Move up/down buttons
                if ui.small_button("▲").on_hover_text("Move up").clicked() {
                    self.move_selected_up();
                }
                if ui.small_button("▼").on_hover_text("Move down").clicked() {
                    self.move_selected_down();
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui: &mut egui::Ui| {
            let tree = self.tree.clone();
            for node in tree.iter().rev() {
                self.render_node(ui, node);
            }
        });

        // ─── Process drag-and-drop via manual position tracking ───
        if let Some(ref drag_id) = self.dragging_id.clone() {
            if pointer_released {
                // Find which item we're over
                if let Some(pos) = pointer_pos {
                    let mut drop_target: Option<(String, ReorderDir)> = None;
                    for (id, rect) in &self.item_rects {
                        if id == drag_id { continue; }
                        if rect.contains(pos) {
                            let mid = rect.center().y;
                            let dir = if pos.y < mid { ReorderDir::Before } else { ReorderDir::After };
                            drop_target = Some((id.clone(), dir));
                            break;
                        }
                    }
                    if let Some((target_id, dir)) = drop_target {
                        self.pending_reorder = Some((drag_id.clone(), target_id, dir));
                    }
                }
                self.dragging_id = None;
            } else if !pointer_down {
                // Mouse released outside — cancel
                self.dragging_id = None;
            } else {
                // Draw drop indicator
                if let Some(pos) = pointer_pos {
                    for (id, rect) in &self.item_rects {
                        if id == drag_id { continue; }
                        if rect.contains(pos) {
                            let mid = rect.center().y;
                            let y = if pos.y < mid { rect.min.y } else { rect.max.y };
                            ui.painter().hline(
                                rect.x_range(), y,
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(79, 195, 247)),
                            );
                            break;
                        }
                    }
                }
            }
        }
    }

    fn render_node(&mut self, ui: &mut egui::Ui, node: &LayerNode) {
        let has_children = !node.children.is_empty();
        let is_selected = self.selected_id.as_deref() == Some(&node.id) && !node.id.is_empty();
        let indent = node.depth as f32 * 16.0;
        let is_dragging = self.dragging_id.as_deref() == Some(&node.id);

        let bg = if is_dragging {
            egui::Color32::from_rgba_unmultiplied(79, 195, 247, 50)
        } else if is_selected {
            egui::Color32::from_rgba_unmultiplied(79, 195, 247, 30)
        } else {
            egui::Color32::TRANSPARENT
        };

        let frame_resp = egui::Frame::none()
            .fill(bg)
            .inner_margin(egui::Margin::symmetric(4.0, 2.0))
            .show(ui, |ui: &mut egui::Ui| {
                ui.horizontal(|ui: &mut egui::Ui| {
                    ui.add_space(indent);

                    // Expand/collapse
                    if has_children {
                        let arrow = if node.expanded { "▼" } else { "▶" };
                        if ui.small_button(arrow).clicked() {
                            toggle_expanded(&mut self.tree, &node.id, &node.name, node.depth);
                        }
                    } else {
                        ui.add_space(20.0);
                    }

                    // Visibility
                    let vis = if node.visible { "👁" } else { "○" };
                    if ui.small_button(vis).clicked() && !node.id.is_empty() {
                        self.pending_toggle = Some((node.id.clone(), !node.visible));
                    }

                    // Icon
                    let icon = match node.tag.as_str() {
                        "g" => "📁", "rect" => "⬜", "circle"|"ellipse" => "⭕",
                        "path" => "✏", "text" => "T", "image" => "🖼",
                        "line"|"polyline"|"polygon" => "📐", _ => "•",
                    };
                    ui.small(icon);

                    // Name — use a button-like interactive label
                    let name = if !node.name.is_empty() && node.name != node.tag {
                        node.name.clone()
                    } else {
                        format!("<{}>", node.tag)
                    };
                    let color = if node.visible {
                        egui::Color32::from_rgb(220, 220, 220)
                    } else {
                        egui::Color32::from_rgb(100, 100, 100)
                    };

                    let resp = ui.add(
                        egui::Button::new(egui::RichText::new(&name).color(color).size(12.0))
                            .frame(false)
                            .sense(egui::Sense::click_and_drag())
                    );

                    if resp.clicked() && !node.id.is_empty() {
                        self.selected_id = Some(node.id.clone());
                        self.pending_select = Some(node.id.clone());
                    }

                    if resp.drag_started() && !node.id.is_empty() {
                        self.dragging_id = Some(node.id.clone());
                    }
                });
            });

        // Store rect for this item (for manual drop target detection)
        if !node.id.is_empty() {
            self.item_rects.push((node.id.clone(), frame_resp.response.rect));
        }

        // Children
        if has_children && node.expanded {
            for child in node.children.iter().rev() {
                self.render_node(ui, child);
            }
        }
    }

    fn move_selected_up(&mut self) {
        if let Some(ref sel) = self.selected_id {
            // Find the previous sibling at the same level
            if let Some((prev_id, _)) = self.find_adjacent(sel, -1) {
                self.pending_reorder = Some((sel.clone(), prev_id, ReorderDir::Before));
            }
        }
    }

    fn move_selected_down(&mut self) {
        if let Some(ref sel) = self.selected_id {
            if let Some((next_id, _)) = self.find_adjacent(sel, 1) {
                self.pending_reorder = Some((sel.clone(), next_id, ReorderDir::After));
            }
        }
    }

    /// Find an adjacent sibling. direction: -1 for previous, 1 for next.
    fn find_adjacent(&self, id: &str, direction: i32) -> Option<(String, usize)> {
        fn search(nodes: &[LayerNode], id: &str, dir: i32) -> Option<(String, usize)> {
            for (i, n) in nodes.iter().enumerate() {
                if n.id == id {
                    let target = i as i32 + dir;
                    if target >= 0 && (target as usize) < nodes.len() {
                        let t = &nodes[target as usize];
                        if !t.id.is_empty() {
                            return Some((t.id.clone(), target as usize));
                        }
                    }
                    return None;
                }
                if let Some(result) = search(&n.children, id, dir) {
                    return Some(result);
                }
            }
            None
        }
        search(&self.tree, id, direction)
    }
}

// ─── Tree helpers (free functions to avoid borrow issues) ───

fn collect_expanded(nodes: &[LayerNode]) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for n in nodes {
        if n.expanded && !n.id.is_empty() { set.insert(n.id.clone()); }
        set.extend(collect_expanded(&n.children));
    }
    set
}

fn restore_expanded(nodes: &mut [LayerNode], expanded: &std::collections::HashSet<String>) {
    for n in nodes.iter_mut() {
        if !n.id.is_empty() && expanded.contains(&n.id) { n.expanded = true; }
        restore_expanded(&mut n.children, expanded);
    }
}

fn toggle_expanded(nodes: &mut [LayerNode], id: &str, name: &str, depth: usize) {
    for n in nodes.iter_mut() {
        if (!id.is_empty() && n.id == id) || (id.is_empty() && n.name == name && n.depth == depth) {
            n.expanded = !n.expanded;
            return;
        }
        toggle_expanded(&mut n.children, id, name, depth);
    }
}
