use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum BrowserMode {
    Save,
    OpenReplace,
    OpenAdditive,
}

struct DirEntry {
    name: String,
    is_dir: bool,
}

pub struct FileBrowser {
    pub open: bool,
    mode: BrowserMode,
    current_dir: PathBuf,
    entries: Vec<DirEntry>,
    filename: String,
    pub result: Option<(BrowserMode, PathBuf)>,
}

impl FileBrowser {
    pub fn new(initial_dir: PathBuf) -> Self {
        let mut fb = Self {
            open: false,
            mode: BrowserMode::OpenReplace,
            current_dir: initial_dir,
            entries: vec![],
            filename: String::new(),
            result: None,
        };
        fb.refresh();
        fb
    }

    pub fn open_save(&mut self) {
        self.mode = BrowserMode::Save;
        self.filename.clear();
        self.result = None;
        self.open = true;
    }

    pub fn open_load_replace(&mut self) {
        self.mode = BrowserMode::OpenReplace;
        self.filename.clear();
        self.result = None;
        self.open = true;
    }

    pub fn open_load_additive(&mut self) {
        self.mode = BrowserMode::OpenAdditive;
        self.filename.clear();
        self.result = None;
        self.open = true;
    }

    pub fn take_result(&mut self) -> Option<(BrowserMode, PathBuf)> {
        self.result.take()
    }

    fn refresh(&mut self) {
        self.entries.clear();
        let Ok(iter) = std::fs::read_dir(&self.current_dir) else { return };
        let mut dirs = vec![];
        let mut files = vec![];
        for entry in iter.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if !name.starts_with('.') {
                    dirs.push(name);
                }
            } else if name.ends_with(".config.json") {
                files.push(name);
            }
        }
        dirs.sort();
        files.sort();
        for d in dirs {
            self.entries.push(DirEntry { name: d, is_dir: true });
        }
        for f in files {
            self.entries.push(DirEntry { name: f, is_dir: false });
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open { return; }

        let title = match self.mode {
            BrowserMode::Save        => "Save Config",
            BrowserMode::OpenReplace => "Open Config (Replace)",
            BrowserMode::OpenAdditive => "Open Config (Additive)",
        };

        let mut open = self.open;
        egui::Window::new(title)
            .open(&mut open)
            .resizable(true)
            .default_size([480.0, 360.0])
            .show(ctx, |ui| {
                // Breadcrumb path
                let mut navigate_to: Option<PathBuf> = None;
                ui.horizontal(|ui| {
                    let components: Vec<_> = self.current_dir.components().collect();
                    let mut path = PathBuf::new();
                    for (i, c) in components.iter().enumerate() {
                        path.push(c);
                        let label = c.as_os_str().to_string_lossy().to_string();
                        if ui.small_button(&label).clicked() {
                            navigate_to = Some(path.clone());
                        }
                        if i + 1 < components.len() {
                            ui.label("/");
                        }
                    }
                });
                if let Some(p) = navigate_to {
                    self.current_dir = p;
                    self.refresh();
                }

                ui.separator();

                // File list
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    // Parent dir entry
                    if self.current_dir.parent().is_some() {
                        if ui.selectable_label(false, "📁 ..").clicked() {
                            if let Some(parent) = self.current_dir.parent() {
                                self.current_dir = parent.to_path_buf();
                                self.refresh();
                            }
                        }
                    }

                    let entries: Vec<(String, bool)> = self.entries.iter()
                        .map(|e| (e.name.clone(), e.is_dir))
                        .collect();

                    for (name, is_dir) in entries {
                        let icon = if is_dir { "📁" } else { "📄" };
                        let label = format!("{} {}", icon, name);
                        if ui.selectable_label(false, &label).clicked() {
                            if is_dir {
                                self.current_dir.push(&name);
                                self.refresh();
                            } else {
                                // Strip .config.json suffix for the filename field
                                self.filename = name.strip_suffix(".config.json")
                                    .unwrap_or(&name)
                                    .to_string();
                                if matches!(self.mode, BrowserMode::OpenReplace | BrowserMode::OpenAdditive) {
                                    let path = self.current_dir.join(&name);
                                    self.result = Some((self.mode, path));
                                    self.open = false;
                                }
                            }
                        }
                    }
                });

                ui.separator();

                // Filename input (save mode only)
                if matches!(self.mode, BrowserMode::Save) {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.filename);
                    });
                    ui.horizontal(|ui| {
                        let can_save = !self.filename.trim().is_empty();
                        if ui.add_enabled(can_save, egui::Button::new("Save")).clicked() {
                            let bare = self.filename.trim().to_string();
                            let filename = format!("{}.config.json", bare);
                            let path = self.current_dir.join(filename);
                            self.result = Some((self.mode, path));
                            self.open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                    });
                }
            });

        self.open = open;
    }
}
