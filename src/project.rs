use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
}

/// Discovers all .svg files in the project directory (non-recursive).
pub fn discover_svg_files(project_dir: &Path) -> Vec<FileInfo> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("svg") {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    files.push(FileInfo {
                        path: path.clone(),
                        name: name.to_string(),
                    });
                }
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Reads an SVG file.
pub fn read_svg(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))
}

/// Writes SVG content to a file.
pub fn write_svg(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    std::fs::write(path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Picks the first SVG file in the project.
pub fn pick_active_file(project_dir: &Path) -> Option<PathBuf> {
    discover_svg_files(project_dir)
        .first()
        .map(|f| f.path.clone())
}

/// Creates a new project with a default SVG.
pub fn create_new_project(dir: &Path, mode: &str) {
    std::fs::create_dir_all(dir).expect("Failed to create project directory");

    let svg = match mode {
        "animated" => include_str!("../examples/animated-project/scene.svg"),
        _ => include_str!("../examples/static-project/drawing.svg"),
    };

    let filename = if mode == "animated" { "scene.svg" } else { "drawing.svg" };
    let path = dir.join(filename);
    if !path.exists() {
        std::fs::write(&path, svg).expect("Failed to create default SVG");
    }
}
