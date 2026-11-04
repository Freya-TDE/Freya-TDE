//! Window "Neues Projekt": the wizard in pages (kind → name and folder →
//! target systems → UI → components → Git and summary). The pages only
//! fill a [`NewProject`]; checks, preview and creating are in
//! [`crate::wizard`].

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::dialog;
use tdr_core::schema::Driver;
use tdr_core::{Platform, ProjectKind};

use super::{Ide, with, with0};
use crate::wizard::{self, NO_BACKEND, NewProject, Starter};

const W: i32 = 600;
const H: i32 = 470;
const M: i32 = 16;
const ROW: i32 = 32;
const BAR: i32 = 48;

/// Pages in order; `Ui` is skipped for libraries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Kind,
    Place,
    Platforms,
    Ui,
    Components,
    Summary,
}

const PAGES: [Page; 6] = [
    Page::Kind,
    Page::Place,
    Page::Platforms,
    Page::Ui,
    Page::Components,
    Page::Summary,
];

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Kind => "Was soll entstehen?",
            Page::Place => "Name und Ort",
            Page::Platforms => "Zielsysteme",
            Page::Ui => "Oberfläche",
            Page::Components => "Komponenten",
            Page::Summary => "Git und Zusammenfassung",
        }
    }
}

const PLATFORMS: [(Platform, &str); 6] = [
    (Platform::Linux, "Linux"),
    (Platform::Windows, "Windows"),
    (Platform::MacOs, "macOS"),
    (Platform::Android, "Android"),
    (Platform::Ios, "iOS"),
    (Platform::Web, "Web (WebAssembly)"),
];

const DRIVERS: [(Driver, &str, &str); 3] = [
    (
        Driver::Postgres,
        "PostgreSQL",
        "postgres://user@localhost/app",
    ),
    (
        Driver::MySql,
        "MySQL / MariaDB",
        "mysql://user@localhost/app",
    ),
    (Driver::Sqlite, "SQLite", "sqlite://app.db"),
];

/// The developer's system (preselected target).
fn this_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Linux
    }
}

pub struct WizardWindow {
    pub window: RnclWindow,
    title: RnclLabel,
    pages: Vec<RnclPanel>,
    // Art
    kind: RnclComboBox,
    kind_hint: RnclLabel,
    // Name und Ort
    name: RnclTextField,
    parent: RnclTextField,
    browse: RnclButton,
    rust: RnclTextField,
    place_hint: RnclLabel,
    // Zielsysteme
    platforms: Vec<RnclCheckBox>,
    // Oberfläche
    backend: RnclComboBox,
    notice: RnclTextArea,
    accept: RnclCheckBox,
    /// Backend names in the order of the combo box.
    backends: RefCell<Vec<String>>,
    // Komponenten
    database: RnclCheckBox,
    driver: RnclComboBox,
    db_url: RnclTextField,
    redis: RnclCheckBox,
    redis_url: RnclTextField,
    web: RnclCheckBox,
    runtime: RnclCheckBox,
    // Git und Zusammenfassung
    git: RnclCheckBox,
    commit: RnclCheckBox,
    summary: RnclTextArea,
    // Leiste
    cancel: RnclButton,
    back: RnclButton,
    next: RnclButton,
    finish: RnclButton,
    page: Cell<usize>,
    /// All installed packs, loaded when the wizard opens.
    registry: RefCell<tdr_core::TypeRegistry>,
}

fn label<P: AsWidget + ?Sized>(parent: &P, text: &str, x: i32, y: i32, w: i32) -> RnclLabel {
    let l = RnclLabel::new(parent, text);
    l.set_geometry(x, y, w, ROW);
    l
}

impl WizardWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Neues Projekt");
        window.set_size(W, H);
        let title = RnclLabel::new(&window, "");
        title.set_geometry(M, 8, W - 2 * M, ROW);
        let inner = W - 2 * M;
        let pages: Vec<RnclPanel> = PAGES
            .iter()
            .map(|_| {
                let p = RnclPanel::new(&window);
                p.set_geometry(M, 8 + ROW + 8, inner, H - BAR - ROW - 24);
                p
            })
            .collect();
        let lw = 150;
        let fx = lw + 8;
        let fw = inner - fx;

        // Art
        let p = &pages[0];
        label(p, "Art", 0, 0, lw);
        let kind = RnclComboBox::new(p);
        kind.set_items(&[
            "Programm (mit Oberfläche)",
            "Bibliothek (Datenmodule, ohne Oberfläche)",
        ]);
        kind.set_current_index(0);
        kind.set_geometry(fx, 0, fw, ROW);
        let kind_hint = RnclLabel::new(p, "");
        kind_hint.set_geometry(0, ROW + 12, inner, 120);

        // Name und Ort
        let p = &pages[1];
        label(p, "Name", 0, 0, lw);
        let name = RnclTextField::new(p);
        name.set_placeholder("z. B. laden (Cargo-Paketname)");
        name.set_geometry(fx, 0, fw, ROW);
        label(p, "Übergeordneter Ordner", 0, ROW + 8, lw);
        let parent = RnclTextField::new(p);
        parent.set_geometry(fx, ROW + 8, fw - 40, ROW);
        let browse = RnclButton::new(p, "…");
        browse.set_geometry(inner - 34, ROW + 8, 34, ROW);
        label(p, "Rust-Version", 0, 2 * (ROW + 8), lw);
        let rust = RnclTextField::new(p);
        rust.set_geometry(fx, 2 * (ROW + 8), 120, ROW);
        let place_hint = RnclLabel::new(p, "");
        place_hint.set_geometry(0, 3 * (ROW + 8) + 4, inner, 80);

        // Zielsysteme
        let p = &pages[2];
        label(
            p,
            "Für welche Systeme? Der Designer zeigt das erste; Unterschiede werden zu cfg-Zweigen.",
            0,
            0,
            inner,
        );
        let platforms = PLATFORMS
            .iter()
            .enumerate()
            .map(|(i, (platform, text))| {
                let c = RnclCheckBox::new(p, text);
                c.set_geometry(0, ROW + 8 + i as i32 * (ROW + 2), inner, ROW);
                c.set_checked(*platform == this_platform());
                c
            })
            .collect();

        // Oberfläche
        let p = &pages[3];
        label(p, "UI-Toolkit", 0, 0, lw);
        let backend = RnclComboBox::new(p);
        backend.set_geometry(fx, 0, fw, ROW);
        let notice = RnclTextArea::new(p);
        notice.set_read_only(true);
        notice.set_word_wrap(true);
        notice.set_geometry(0, ROW + 10, inner, 190);
        let accept = RnclCheckBox::new(p, "Lizenzhinweis gelesen, Projekt trotzdem anlegen");
        accept.set_geometry(0, ROW + 210, inner, ROW);

        // Komponenten
        let p = &pages[4];
        label(
            p,
            "Im Datenmodul „services“ anlegen (Tokio kommt bei Bedarf dazu):",
            0,
            0,
            inner,
        );
        let database = RnclCheckBox::new(p, "Datenbank (SQLx)");
        database.set_geometry(0, ROW + 6, lw + 40, ROW);
        let driver = RnclComboBox::new(p);
        driver.set_items(&DRIVERS.map(|d| d.1));
        driver.set_current_index(0);
        driver.set_geometry(fx + 40, ROW + 6, fw - 40, ROW);
        let db_url = RnclTextField::new(p);
        db_url.set_text(DRIVERS[0].2);
        db_url.set_geometry(fx + 40, 2 * ROW + 10, fw - 40, ROW);
        let redis = RnclCheckBox::new(p, "Redis / Valkey");
        redis.set_geometry(0, 3 * ROW + 20, lw + 40, ROW);
        let redis_url = RnclTextField::new(p);
        redis_url.set_text("redis://127.0.0.1/");
        redis_url.set_geometry(fx + 40, 3 * ROW + 20, fw - 40, ROW);
        let web = RnclCheckBox::new(p, "Webserver (Axum)");
        web.set_geometry(0, 4 * ROW + 32, inner, ROW);
        let runtime = RnclCheckBox::new(p, "Tokio-Laufzeit für eigenen async-Code");
        runtime.set_geometry(0, 5 * ROW + 38, inner, ROW);

        // Git und Zusammenfassung
        let p = &pages[5];
        let git = RnclCheckBox::new(p, "Git-Repository anlegen (.gitignore mit /target)");
        git.set_geometry(0, 0, inner, ROW);
        git.set_checked(true);
        let commit = RnclCheckBox::new(p, "… und das neue Projekt committen");
        commit.set_geometry(24, ROW + 2, inner - 24, ROW);
        let summary = RnclTextArea::new(p);
        summary.set_read_only(true);
        summary.set_monospace(true);
        summary.set_geometry(0, 2 * ROW + 12, inner, H - BAR - ROW - 24 - 2 * ROW - 12);

        // Leiste
        let y = H - BAR + 8;
        let cancel = RnclButton::new(&window, "Abbrechen");
        cancel.set_geometry(M, y, 110, ROW);
        let back = RnclButton::new(&window, "< Zurück");
        back.set_geometry(W - M - 3 * 110 - 16, y, 110, ROW);
        let next = RnclButton::new(&window, "Weiter >");
        next.set_geometry(W - M - 2 * 110 - 8, y, 110, ROW);
        let finish = RnclButton::new(&window, "Anlegen");
        finish.set_geometry(W - M - 110, y, 110, ROW);

        Self {
            window,
            title,
            pages,
            kind,
            kind_hint,
            name,
            parent,
            browse,
            rust,
            place_hint,
            platforms,
            backend,
            notice,
            accept,
            backends: RefCell::new(Vec::new()),
            database,
            driver,
            db_url,
            redis,
            redis_url,
            web,
            runtime,
            git,
            commit,
            summary,
            cancel,
            back,
            next,
            finish,
            page: Cell::new(0),
            registry: RefCell::new(tdr_core::TypeRegistry::with_builtins()),
        }
    }

    fn library(&self) -> bool {
        self.kind.current_index() == 1
    }

    /// The pages shown for the chosen kind.
    fn visible_pages(&self) -> Vec<usize> {
        (0..PAGES.len())
            .filter(|&i| !(self.library() && PAGES[i] == Page::Ui))
            .collect()
    }

    /// The request as the pages describe it.
    pub fn request(&self) -> NewProject {
        let parent = PathBuf::from(self.parent.text().trim());
        let name = self.name.text().trim().to_owned();
        let mut request = if self.library() {
            NewProject::library(&name, &parent)
        } else {
            let backends = self.backends.borrow();
            let backend = usize::try_from(self.backend.current_index())
                .ok()
                .and_then(|i| backends.get(i))
                .cloned()
                .unwrap_or_else(|| NO_BACKEND.to_owned());
            NewProject::program(&name, &parent, &backend)
        };
        request.rust_version = self.rust.text().trim().to_owned();
        request.platforms = PLATFORMS
            .iter()
            .zip(&self.platforms)
            .filter(|(_, c)| c.is_checked())
            .map(|((p, _), _)| p.clone())
            .collect();
        if self.database.is_checked() {
            let driver = usize::try_from(self.driver.current_index())
                .ok()
                .and_then(|i| DRIVERS.get(i))
                .map_or(Driver::Postgres, |d| d.0);
            request.starters.push(Starter::Database {
                driver,
                url: self.db_url.text().trim().to_owned(),
            });
        }
        if self.redis.is_checked() {
            request.starters.push(Starter::Redis {
                url: self.redis_url.text().trim().to_owned(),
            });
        }
        if self.web.is_checked() {
            request.starters.push(Starter::WebServer);
        }
        if self.runtime.is_checked() {
            request.starters.push(Starter::Runtime);
        }
        request.git = self.git.is_checked();
        request.initial_commit = request.git && self.commit.is_checked();
        request.license_accepted = self.accept.is_checked();
        request
    }

    /// Fills the pages from `request` (automation, tests).
    pub fn set_request(&self, request: &NewProject) {
        self.kind
            .set_current_index(i32::from(request.kind == ProjectKind::Library));
        self.name.set_text(&request.name);
        self.parent.set_text(&request.parent.to_string_lossy());
        self.rust.set_text(&request.rust_version);
        for ((platform, _), check) in PLATFORMS.iter().zip(&self.platforms) {
            check.set_checked(request.platforms.contains(platform));
        }
        if let Some(i) = self
            .backends
            .borrow()
            .iter()
            .position(|b| *b == request.backend)
        {
            self.backend.set_current_index(i as i32);
        }
        self.database.set_checked(false);
        self.redis.set_checked(false);
        self.web.set_checked(false);
        self.runtime.set_checked(false);
        for starter in &request.starters {
            match starter {
                Starter::Database { driver, url } => {
                    self.database.set_checked(true);
                    if let Some(i) = DRIVERS.iter().position(|d| d.0 == *driver) {
                        self.driver.set_current_index(i as i32);
                    }
                    self.db_url.set_text(url);
                }
                Starter::Redis { url } => {
                    self.redis.set_checked(true);
                    self.redis_url.set_text(url);
                }
                Starter::WebServer => self.web.set_checked(true),
                Starter::Runtime => self.runtime.set_checked(true),
            }
        }
        self.git.set_checked(request.git);
        self.commit.set_checked(request.initial_commit);
        self.accept.set_checked(request.license_accepted);
    }

    /// Starts over: defaults, first page, installed backends.
    fn reset(&self, ide: &Ide) {
        let registry = super::full_registry(&ide.pack_dirs).0;
        *self.registry.borrow_mut() = registry.clone();
        let mut names: Vec<String> = registry.backends().map(|b| b.name.clone()).collect();
        names.sort_by_key(|n| {
            // Reihenfolge wie beschlossen: GTK 4, egui, iced, Slint, Flutter, dann der Rest.
            ["gtk4", "egui", "iced", "slint", "flutter"]
                .iter()
                .position(|k| k == n)
                .unwrap_or(99)
        });
        let labels: Vec<String> = names
            .iter()
            .map(
                |n| match registry.backend(n).and_then(|b| b.license_notice.as_ref()) {
                    Some(_) => format!("{n} (Lizenzhinweis)"),
                    None => n.clone(),
                },
            )
            .chain(std::iter::once(
                "Ohne Oberfläche (nur Komponenten)".to_owned(),
            ))
            .collect();
        names.push(NO_BACKEND.to_owned());
        self.backend.set_items(&labels);
        self.backend.set_current_index(0);
        *self.backends.borrow_mut() = names;
        let parent = ide
            .session()
            .project_dir()
            .and_then(|d| d.parent().map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_default();
        let request = NewProject::program("", &parent, &self.backends.borrow()[0]);
        self.set_request(&NewProject {
            platforms: vec![this_platform()],
            git: true,
            initial_commit: true,
            ..request
        });
        self.page.set(0);
    }

    /// Shows the current page and updates hints, notice and summary.
    fn refresh(&self, _ide: &Ide) {
        let visible = self.visible_pages();
        let position = visible
            .iter()
            .position(|&p| p == self.page.get())
            .unwrap_or(0);
        let page = visible[position];
        self.page.set(page);
        for (i, panel) in self.pages.iter().enumerate() {
            panel.widget().set_visible(i == page);
        }
        self.title.set_text(&format!(
            "<b>Schritt {} von {}: {}</b>",
            position + 1,
            visible.len(),
            PAGES[page].title()
        ));
        self.back.widget().set_enabled(position > 0);
        self.next.widget().set_enabled(position + 1 < visible.len());

        let request = self.request();
        let plan = wizard::plan(&request, &self.registry.borrow());
        self.kind_hint.set_text(if self.library() {
            "Eine Bibliothek (lib.rs) mit Datenmodulen: Komponenten wie Datenbank, Redis oder\n\
             Webserver als öffentlicher Service-Container App, den andere Crates nutzen.\n\
             Keine Oberfläche; async-Komponenten laufen auf der Tokio-Laufzeit des Aufrufers."
        } else {
            "Ein Programm (main.rs) mit Formularen für ein UI-Toolkit und optional\n\
             Datenmodulen für Datenbank, Redis, Webserver …"
        });
        self.place_hint
            .set_text(&format!("Projektordner: {}", plan.dir.display()));
        match &plan.license_notice {
            Some(text) => {
                self.notice.set_text(text);
                self.notice.widget().set_visible(true);
                self.accept.widget().set_visible(true);
            }
            None => {
                self.notice.set_text(
                    "Weitere Toolkits (WebView, Flutter) kommen als eigene Backend-Packs dazu.",
                );
                self.accept.widget().set_visible(false);
            }
        }
        self.driver.widget().set_enabled(self.database.is_checked());
        self.db_url.widget().set_enabled(self.database.is_checked());
        self.redis_url.widget().set_enabled(self.redis.is_checked());
        self.commit.widget().set_enabled(self.git.is_checked());
        self.summary.set_text(&summary(&request, &plan));
        self.finish.widget().set_enabled(plan.is_ok());
    }
}

/// The text of the summary page.
fn summary(request: &NewProject, plan: &wizard::Plan) -> String {
    let mut text = String::new();
    if !plan.errors.is_empty() {
        text.push_str("Noch zu klären:\n");
        for e in &plan.errors {
            text.push_str(&format!("  ✗ {} ({})\n", e.detail, e.code));
        }
        text.push('\n');
    }
    let kind = match request.kind {
        ProjectKind::Binary => format!("Programm, Oberfläche {}", request.backend),
        ProjectKind::Library => "Bibliothek".to_owned(),
    };
    text.push_str(&format!(
        "{} ({kind}, Rust {})\n{}\n\n",
        request.name,
        request.rust_version,
        plan.dir.display()
    ));
    if !plan.components.is_empty() {
        text.push_str("Komponenten in services:\n");
        for c in &plan.components {
            text.push_str(&format!("  {c}\n"));
        }
        text.push('\n');
    }
    text.push_str("Dateien:\n");
    for f in &plan.files {
        text.push_str(&format!("  {f}\n"));
    }
    text
}

impl Ide {
    /// Shows the new-project wizard (File → New project).
    pub fn show_wizard(&self) {
        self.wizard.reset(self);
        self.wizard.refresh(self);
        self.wizard.window.show();
    }

    /// The wizard's current request (automation).
    pub fn wizard_request(&self) -> NewProject {
        self.wizard.request()
    }

    /// Fills the wizard with `request` (automation).
    pub fn set_wizard_request(&self, request: &NewProject) {
        self.wizard.set_request(request);
        self.wizard.refresh(self);
    }

    /// The wizard window (screenshots, tests).
    pub fn wizard_window(&self) -> &RnclWindow {
        &self.wizard.window
    }

    /// Goes to the next (`true`) or previous page of the wizard.
    pub fn wizard_step(&self, forward: bool) {
        let visible = self.wizard.visible_pages();
        let position = visible
            .iter()
            .position(|&p| p == self.wizard.page.get())
            .unwrap_or(0);
        let target = if forward {
            (position + 1).min(visible.len() - 1)
        } else {
            position.saturating_sub(1)
        };
        self.wizard.page.set(visible[target]);
        self.wizard.refresh(self);
    }

    /// Creates the project described by `request`, adds it to the project
    /// list and opens it.
    pub fn create_project(
        &self,
        request: &NewProject,
    ) -> Result<wizard::Created, Vec<wizard::WizardIssue>> {
        if !self.confirm_discard() {
            return Err(vec![wizard::WizardIssue {
                code: "cancelled".to_owned(),
                detail: "Ungespeicherte Änderungen behalten".to_owned(),
            }]);
        }
        let registry = super::full_registry(&self.pack_dirs).0;
        let created = wizard::create(request, &registry)?;
        if self.projects.borrow().get(&created.ptdr).is_none() {
            let _ = self.projects.borrow_mut().add(&created.ptdr, None);
            self.save_projects();
        }
        self.open_path(&created.ptdr);
        let mut status = format!("Projekt {} angelegt.", request.name);
        for w in &created.warnings {
            status.push_str(&format!(" {} ({})", w.detail, w.code));
        }
        self.set_status(&status);
        Ok(created)
    }

    /// "Anlegen" in the wizard.
    fn wizard_finish(&self) {
        let request = self.wizard.request();
        match self.create_project(&request) {
            Ok(_) => {
                self.wizard.window.hide();
                if let Some(ide) = self.me.upgrade() {
                    self.main.build_palette(&ide);
                }
            }
            Err(errors) => {
                let text: Vec<String> = errors
                    .iter()
                    .map(|e| format!("{} ({})", e.detail, e.code))
                    .collect();
                dialog::error(
                    Some(self.wizard.window.widget()),
                    "Neues Projekt",
                    &text.join("\n"),
                );
            }
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let w = &ide.wizard;
    w.cancel
        .on_clicked(with0(ide, |ide| ide.wizard.window.hide()));
    w.back.on_clicked(with0(ide, |ide| ide.wizard_step(false)));
    w.next.on_clicked(with0(ide, |ide| ide.wizard_step(true)));
    w.finish.on_clicked(with0(ide, |ide| ide.wizard_finish()));
    w.browse.on_clicked(with0(ide, |ide| {
        if let Some(dir) = dialog::choose_folder(
            Some(ide.wizard.window.widget()),
            "Übergeordneten Ordner wählen",
        ) {
            ide.wizard.parent.set_text(&dir.to_string_lossy());
            ide.wizard.refresh(ide);
        }
    }));
    // Jede Eingabe aktualisiert Hinweise und Zusammenfassung.
    for field in [&w.name, &w.parent, &w.rust, &w.db_url, &w.redis_url] {
        field.on_text_changed(with(ide, |ide, _: String| ide.wizard.refresh(ide)));
    }
    for combo in [&w.kind, &w.backend] {
        combo.on_current_index_changed(with(ide, |ide, _: i32| ide.wizard.refresh(ide)));
    }
    w.driver
        .on_current_index_changed(with(ide, |ide, index: i32| {
            // Beispiel-URL zum Treiber, solange die alte unverändert ist.
            let url = ide.wizard.db_url.text();
            if DRIVERS.iter().any(|d| d.2 == url)
                && let Some(d) = usize::try_from(index).ok().and_then(|i| DRIVERS.get(i))
            {
                ide.wizard.db_url.set_text(d.2);
            }
            ide.wizard.refresh(ide);
        }));
    let checks: Vec<&RnclCheckBox> = [
        &w.accept,
        &w.database,
        &w.redis,
        &w.web,
        &w.runtime,
        &w.git,
        &w.commit,
    ]
    .into_iter()
    .chain(&w.platforms)
    .collect();
    for check in checks {
        check.on_toggled(with(ide, |ide, _: bool| ide.wizard.refresh(ide)));
    }
}
