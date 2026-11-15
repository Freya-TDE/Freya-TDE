//! Window "Einstellungen" (File → Settings, Ctrl+,): tab "Allgemein"
//! (working directory, language, appearance) and tab "KI" (profiles, default
//! profile, profile of the current project, AI window). The window edits a
//! [`SettingsEdit`]; only "OK" stores keys, saves and applies
//! ([`Ide::update_settings`], also for automation).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Application, ColorScheme, dialog};

use super::{Ide, with, with0};
use crate::codetools::MethodOrder;
use crate::settings::{
    AiProfile, AiProvider, Language, SecretStore, Settings, SettingsEdit, SettingsError, Theme,
    codes,
};

const W: i32 = 660;
const H: i32 = 560;
const M: i32 = 16;
const ROW: i32 = 32;
const GAP: i32 = 8;
const BAR: i32 = 48;
const LABEL_W: i32 = 150;
const LIST_W: i32 = 180;

pub struct SettingsWindow {
    pub window: RnclWindow,
    tabs: RnclTabView,
    general: RnclPanel,
    ai: RnclPanel,
    // Allgemein
    base_dir: RnclTextField,
    browse: RnclButton,
    base_hint: RnclLabel,
    language: RnclComboBox,
    language_hint: RnclLabel,
    theme: RnclComboBox,
    method_order: RnclComboBox,
    general_labels: Vec<RnclLabel>,
    // KI
    profiles: RnclListBox,
    add: RnclButton,
    remove: RnclButton,
    name: RnclTextField,
    provider: RnclComboBox,
    model: RnclTextField,
    endpoint: RnclTextField,
    key: RnclTextField,
    key_state: RnclLabel,
    remove_key: RnclButton,
    default_profile: RnclComboBox,
    project_profile: RnclComboBox,
    show_on_new: RnclCheckBox,
    show_always: RnclCheckBox,
    ai_hint: RnclLabel,
    ai_labels: Vec<RnclLabel>,
    // Leiste
    cancel: RnclButton,
    ok: RnclButton,
    /// What the window edits (taken over on "OK").
    edit: RefCell<SettingsEdit>,
    /// Index of the shown profile (`-1` = none).
    selected: Cell<i32>,
    /// The `.ptdr` of the project the window was opened for.
    project: RefCell<Option<PathBuf>>,
    /// Set while fields are filled (their signals must not edit).
    filling: Cell<bool>,
}

fn label<P: AsWidget + ?Sized>(parent: &P, text: &str) -> RnclLabel {
    RnclLabel::new(parent, text)
}

impl SettingsWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Einstellungen");
        window.set_size(W, H);
        let tabs = RnclTabView::new(&window);
        let general = tabs.add_tab("Allgemein");
        let ai = tabs.add_tab("KI");

        // Allgemein
        let p = &general;
        let general_labels = vec![
            label(p, "Arbeitsverzeichnis"),
            label(p, "Sprache"),
            label(p, "Darstellung"),
            label(p, "Neue Methoden"),
        ];
        let base_dir = RnclTextField::new(p);
        base_dir.set_placeholder("leer = Home-Ordner, z. B. ~/DevArea/RUST");
        let browse = RnclButton::new(p, "…");
        let base_hint = label(
            p,
            "Hier beginnen Dateidialoge und der Assistent für neue Projekte.",
        );
        let language = RnclComboBox::new(p);
        language.set_items(&Language::ALL.map(Language::label));
        let language_hint = label(
            p,
            "Gilt vorerst für den CT-Editor; Freya TDE selbst ist noch deutsch.",
        );
        let theme = RnclComboBox::new(p);
        theme.set_items(&Theme::ALL.map(|t| match t {
            Theme::System => "System (Standard)",
            other => other.label(),
        }));

        let method_order = RnclComboBox::new(p);
        method_order.set_items(&MethodOrder::ALL.map(MethodOrder::label));
        method_order
            .widget()
            .set_tool_tip("Wohin „Code vervollständigen“ (Strg+Umschalt+C) neue Methoden setzt");

        // KI
        let p = &ai;
        let profiles = RnclListBox::new(p);
        let add = RnclButton::new(p, "Neu");
        let remove = RnclButton::new(p, "Entfernen");
        let ai_labels = vec![
            label(p, "Name"),
            label(p, "Anbieter"),
            label(p, "Modell"),
            label(p, "Adresse"),
            label(p, "API-Key"),
            label(p, "Standardprofil"),
            label(p, "Für dieses Projekt"),
        ];
        let name = RnclTextField::new(p);
        let provider = RnclComboBox::new(p);
        provider.set_items(&AiProvider::ALL.map(AiProvider::label));
        let model = RnclTextField::new(p);
        model.set_placeholder("z. B. claude-opus-5-5, llama3.2");
        let endpoint = RnclTextField::new(p);
        let key = RnclTextField::new(p);
        key.set_password_mode(true);
        let key_state = label(p, "");
        let remove_key = RnclButton::new(p, "Key entfernen");
        let default_profile = RnclComboBox::new(p);
        let project_profile = RnclComboBox::new(p);
        let show_on_new = RnclCheckBox::new(p, "KI-Fenster bei neuem Projekt anzeigen");
        let show_always = RnclCheckBox::new(p, "KI-Fenster immer anzeigen");
        let ai_hint = label(p, "");

        let cancel = RnclButton::new(&window, "Abbrechen");
        let ok = RnclButton::new(&window, "OK");

        let w = Self {
            window,
            tabs,
            general,
            ai,
            base_dir,
            browse,
            base_hint,
            language,
            language_hint,
            theme,
            method_order,
            general_labels,
            profiles,
            add,
            remove,
            name,
            provider,
            model,
            endpoint,
            key,
            key_state,
            remove_key,
            default_profile,
            project_profile,
            show_on_new,
            show_always,
            ai_hint,
            ai_labels,
            cancel,
            ok,
            edit: RefCell::new(SettingsEdit::default()),
            selected: Cell::new(-1),
            project: RefCell::new(None),
            filling: Cell::new(false),
        };
        w.place();
        w.layout_pages(W - 2 * M, H - BAR - M - 40);
        w
    }

    /// Places everything for the start size; anchors let fields grow with
    /// the window and keep the buttons at the bottom right.
    fn place(&self) {
        use ruf::Anchors as A;
        let put = |w: &dyn Component, x, y, width, height, anchors| {
            w.set_geometry(x, y, width, height);
            w.set_anchors(anchors);
        };
        let tabs_h = H - BAR - M;
        put(&self.tabs, M, M, W - 2 * M, tabs_h, A::ALL);
        // Leiste
        let y = H - BAR + 8;
        put(
            &self.cancel,
            W - M - 2 * 110 - GAP,
            y,
            110,
            ROW,
            A::BOTTOM_RIGHT,
        );
        put(&self.ok, W - M - 110, y, 110, ROW, A::BOTTOM_RIGHT);
    }

    /// Lays out both tab pages for their real size (the tab view sizes its
    /// pages only after they are filled, so anchors would measure from the
    /// wrong size).
    fn layout_pages(&self, width: i32, height: i32) {
        let p = 12;
        let inner = (width - 2 * p).max(300);
        let fx = p + LABEL_W + GAP;
        let fw = p + inner - fx;
        let line = |i: i32| p + i * (ROW + GAP);

        // Allgemein
        for (l, i) in self.general_labels.iter().zip([0, 2, 4, 5]) {
            l.set_geometry(p, line(i), LABEL_W, ROW);
        }
        self.base_dir.set_geometry(fx, line(0), fw - 34 - GAP, ROW);
        self.browse.set_geometry(p + inner - 34, line(0), 34, ROW);
        self.base_hint.set_geometry(fx, line(1), fw, ROW);
        self.language.set_geometry(fx, line(2), fw, ROW);
        self.language_hint.set_geometry(fx, line(3), fw, ROW);
        self.theme.set_geometry(fx, line(4), fw, ROW);
        self.method_order.set_geometry(fx, line(5), fw, ROW);

        // KI: links die Profile, rechts das gewählte Profil.
        let list_h = 5 * (ROW + GAP) - GAP;
        self.profiles
            .set_geometry(p, line(0), LIST_W, list_h - ROW - GAP);
        let half = (LIST_W - GAP) / 2;
        let y = line(0) + list_h - ROW;
        self.add.set_geometry(p, y, half, ROW);
        self.remove.set_geometry(p + half + GAP, y, half, ROW);
        let rx = p + LIST_W + 3 * GAP;
        let rlw = 90;
        let rfx = rx + rlw + GAP;
        let rfw = p + inner - rfx;
        for (i, l) in self.ai_labels.iter().take(5).enumerate() {
            l.set_geometry(rx, line(i as i32), rlw, ROW);
        }
        self.name.set_geometry(rfx, line(0), rfw, ROW);
        self.provider.set_geometry(rfx, line(1), rfw, ROW);
        self.model.set_geometry(rfx, line(2), rfw, ROW);
        self.endpoint.set_geometry(rfx, line(3), rfw, ROW);
        let kw = rfw - 130 - GAP;
        self.key.set_geometry(rfx, line(4), kw, ROW);
        self.remove_key
            .set_geometry(rfx + kw + GAP, line(4), 130, ROW);
        self.key_state.set_geometry(rfx, line(5), rfw, ROW);
        for (i, l) in self.ai_labels.iter().skip(5).enumerate() {
            l.set_geometry(p, line(6 + i as i32), LABEL_W, ROW);
        }
        self.default_profile.set_geometry(fx, line(6), fw, ROW);
        self.project_profile.set_geometry(fx, line(7), fw, ROW);
        self.show_on_new.set_geometry(p, line(8), inner, ROW);
        self.show_always.set_geometry(p, line(8) + ROW, inner, ROW);
        let y = line(8) + 2 * ROW + GAP;
        self.ai_hint
            .set_geometry(p, y, inner, (height - y - p).max(ROW));
    }

    /// The selected profile in the edit.
    fn profile(&self) -> Option<AiProfile> {
        let i = usize::try_from(self.selected.get()).ok()?;
        self.edit.borrow().settings.ai.profiles.get(i).cloned()
    }

    /// Changes the selected profile in the edit.
    fn change_profile(&self, f: impl FnOnce(&mut AiProfile)) {
        if self.filling.get() {
            return;
        }
        if let Ok(i) = usize::try_from(self.selected.get())
            && let Some(p) = self.edit.borrow_mut().settings.ai.profiles.get_mut(i)
        {
            f(p);
        }
    }

    /// Fills every field from the edit.
    fn fill(&self) {
        self.filling.set(true);
        let edit = self.edit.borrow();
        let s = &edit.settings;
        self.base_dir.set_text(
            &s.base_dir
                .as_ref()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let language = Language::ALL.iter().position(|x| *x == s.language);
        self.language
            .set_current_index(language.unwrap_or(0) as i32);
        let theme = Theme::ALL.iter().position(|x| *x == s.theme);
        self.theme.set_current_index(theme.unwrap_or(0) as i32);
        let order = MethodOrder::ALL.iter().position(|x| *x == s.method_order);
        self.method_order
            .set_current_index(order.unwrap_or(0) as i32);

        let names: Vec<String> = s.ai.profiles.iter().map(|p| p.name.clone()).collect();
        self.profiles.set_items(&names);
        let count = names.len() as i32;
        if self.selected.get() >= count {
            self.selected.set(count - 1);
        }
        if self.selected.get() < 0 && count > 0 {
            self.selected.set(0);
        }
        self.profiles.set_current_index(self.selected.get());
        let mut choices = vec!["(keins)".to_owned()];
        choices.extend(names.iter().cloned());
        self.default_profile.set_items(&choices);
        let pos = |name: Option<&String>| {
            name.and_then(|n| names.iter().position(|x| x == n))
                .map_or(0, |i| i as i32 + 1)
        };
        self.default_profile
            .set_current_index(pos(s.ai.default_profile.as_ref()));
        choices[0] = "Standardprofil".to_owned();
        self.project_profile.set_items(&choices);
        let project = self.project.borrow().clone();
        self.project_profile
            .set_current_index(pos(project.as_ref().and_then(|p| s.ai.projects.get(p))));
        self.show_on_new.set_checked(s.ai.show_on_new_project);
        self.show_always.set_checked(s.ai.show_always);
        drop(edit);
        self.fill_profile();
        self.filling.set(false);
        self.update_state();
    }

    /// Fills the fields of the selected profile.
    fn fill_profile(&self) {
        let was = self.filling.replace(true);
        let p = self.profile();
        for w in [
            self.name.widget(),
            self.provider.widget(),
            self.model.widget(),
            self.endpoint.widget(),
            self.key.widget(),
            self.remove_key.widget(),
            self.remove.widget(),
        ] {
            w.set_enabled(p.is_some());
        }
        let p = p.unwrap_or_else(|| AiProfile::new("", AiProvider::Anthropic));
        self.name.set_text(&p.name);
        self.provider.set_current_index(
            AiProvider::ALL
                .iter()
                .position(|x| *x == p.provider)
                .unwrap_or(0) as i32,
        );
        self.model.set_text(&p.model);
        self.endpoint
            .set_text(p.endpoint.as_deref().unwrap_or_default());
        self.key.set_text(
            self.edit
                .borrow()
                .keys
                .get(&p.name)
                .cloned()
                .flatten()
                .as_deref()
                .unwrap_or_default(),
        );
        self.filling.set(was);
    }

    /// Placeholders, key state and what is enabled.
    fn update_state(&self) {
        let edit = self.edit.borrow();
        let p = self.profile();
        let endpoint = p
            .as_ref()
            .and_then(|p| p.provider.default_endpoint())
            .map_or_else(
                || "Pflicht, z. B. http://localhost:1234/v1".to_owned(),
                |e| format!("leer = {e}"),
            );
        self.endpoint.set_placeholder(&endpoint);
        let state = match &p {
            None => String::new(),
            Some(p) => match edit.keys.get(&p.name) {
                Some(Some(k)) if !k.trim().is_empty() => {
                    "Neuer Key – wird mit OK gespeichert.".to_owned()
                }
                Some(_) => "Key wird bei OK entfernt.".to_owned(),
                None if p.key_in_keyring => "Key im Schlüsselbund.".to_owned(),
                None if p.key.is_some() => {
                    "Key in der Einstellungsdatei (kein Schlüsselbund).".to_owned()
                }
                None if !p.provider.needs_key() => "Kein Key nötig.".to_owned(),
                None => "Noch kein Key.".to_owned(),
            },
        };
        self.key_state.set_text(&state);
        self.key.set_placeholder(
            if p.as_ref()
                .is_some_and(|p| p.has_key() && !edit.keys.contains_key(&p.name))
            {
                "gespeichert – zum Ändern neuen Key eingeben"
            } else {
                "API-Key"
            },
        );
        let connected = edit.settings.ai.connected();
        self.show_on_new.widget().set_enabled(connected);
        self.show_always.widget().set_enabled(connected);
        self.default_profile.widget().set_enabled(connected);
        self.project_profile
            .widget()
            .set_enabled(connected && self.project.borrow().is_some());
        self.ai_hint.set_text(if !connected {
            "Noch keine KI angebunden: „Neu“ legt ein Profil an (Anthropic, OpenAI, Ollama oder\n\
             ein OpenAI-kompatibler Server wie LM Studio). Mehrere Profile sind möglich;\n\
             je Projekt lässt sich eines wählen."
        } else if self.project.borrow().is_none() {
            "Das Profil je Projekt lässt sich wählen, sobald das Projekt gespeichert ist."
        } else {
            "Ohne eigene Wahl nutzt ein Projekt das Standardprofil."
        });
    }
}

impl Ide {
    /// Shows the settings window (File → Settings).
    pub fn show_settings(&self) {
        let w = &self.settings_window;
        *w.edit.borrow_mut() = SettingsEdit::new(&self.settings.borrow());
        *w.project.borrow_mut() = self.path.borrow().clone();
        w.selected.set(-1);
        w.tabs.set_current_index(0);
        w.fill();
        w.window.show();
    }

    /// The settings as currently in effect.
    pub fn settings(&self) -> Settings {
        self.settings.borrow().clone()
    }

    /// The settings window (screenshots, tests).
    pub fn settings_window(&self) -> &RnclWindow {
        &self.settings_window.window
    }

    /// Shows tab `index` of the settings window (0 = Allgemein, 1 = KI).
    pub fn settings_tab(&self, index: i32) {
        self.settings_window.tabs.set_current_index(index);
    }

    /// What the settings window currently edits (automation, tests).
    pub fn settings_edit(&self) -> SettingsEdit {
        self.settings_window.edit.borrow().clone()
    }

    /// Replaces what the settings window edits and refills it.
    pub fn set_settings_edit(&self, edit: SettingsEdit) {
        *self.settings_window.edit.borrow_mut() = edit;
        self.settings_window.fill();
    }

    /// Uses `store` for API keys instead of the system keyring (tests,
    /// automation).
    pub fn set_secret_store(&self, store: Box<dyn SecretStore>) {
        *self.secrets.borrow_mut() = store;
    }

    /// The API key of the AI profile for the current project (keyring,
    /// settings file or `FREYA_TDE_AI_KEY`).
    pub fn ai_key(&self) -> Option<String> {
        let settings = self.settings.borrow();
        let path = self.path.borrow();
        let profile = settings.ai.profile_for(path.as_deref())?;
        settings.key(&profile.name, self.secrets.borrow().as_ref())
    }

    /// Takes over `edit`: stores and removes keys, saves the settings file
    /// and applies the settings. Blocking problems come back with their codes
    /// (a missing key is not one: it can still come from `FREYA_TDE_AI_KEY`;
    /// nor is a key over `http://`, which is only a warning).
    pub fn update_settings(&self, edit: &SettingsEdit) -> Result<Settings, Vec<SettingsError>> {
        let blocking: Vec<SettingsError> = edit
            .settings
            .validate()
            .into_iter()
            .filter(|i| i.code != codes::KEY_MISSING && i.code != codes::INSECURE_KEY_TRANSPORT)
            .map(|i| SettingsError {
                code: i.code,
                detail: i.detail,
            })
            .collect();
        if !blocking.is_empty() {
            return Err(blocking);
        }
        let saved = self.settings.borrow().clone();
        let settings = edit.commit(&saved, self.secrets.borrow().as_ref());
        if let Some(file) = &self.settings_file
            && let Err(e) = settings.save_to(file)
        {
            return Err(vec![e]);
        }
        *self.settings.borrow_mut() = settings.clone();
        self.apply_settings();
        Ok(settings)
    }

    /// Applies appearance and start folder of the current settings.
    pub(super) fn apply_settings(&self) {
        let settings = self.settings.borrow();
        let scheme = match settings.theme {
            Theme::System => ColorScheme::Unknown,
            Theme::Light => ColorScheme::Light,
            Theme::Dark => ColorScheme::Dark,
        };
        Application::set_color_scheme(scheme);
        dialog::set_default_folder(settings.start_folder().as_deref());
    }

    /// "OK" in the settings window.
    fn settings_ok(&self) {
        let edit = self.settings_window.edit.borrow().clone();
        match self.update_settings(&edit) {
            Ok(_) => {
                self.settings_window.window.hide();
                self.set_status("Einstellungen gespeichert.");
            }
            Err(errors) => {
                let text: Vec<String> = errors
                    .iter()
                    .map(|e| format!("{} ({})", e.detail, e.code))
                    .collect();
                dialog::error(
                    Some(self.settings_window.window.widget()),
                    "Einstellungen",
                    &text.join("\n"),
                );
            }
        }
    }

    /// "Neu" on the AI tab: a profile with a free name.
    pub fn settings_add_profile(&self) {
        let w = &self.settings_window;
        {
            let mut edit = w.edit.borrow_mut();
            let profiles = &mut edit.settings.ai.profiles;
            let name = (1..)
                .map(|n| format!("Profil {n}"))
                .find(|n| profiles.iter().all(|p| p.name != *n))
                .expect("free name");
            profiles.push(AiProfile::new(&name, AiProvider::Anthropic));
            w.selected.set(profiles.len() as i32 - 1);
            if edit.settings.ai.default_profile.is_none() {
                edit.settings.ai.default_profile = Some(name);
            }
        }
        w.fill();
    }

    /// "Entfernen" on the AI tab.
    fn settings_remove_profile(&self) {
        let w = &self.settings_window;
        if let Some(p) = w.profile() {
            w.edit.borrow_mut().remove(&p.name);
            w.fill();
        }
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let w = &ide.settings_window;
    for page in [&w.general, &w.ai] {
        let weak = Rc::downgrade(ide);
        page.widget().on_resized(move |width, height| {
            if let Some(ide) = weak.upgrade() {
                ide.settings_window.layout_pages(width, height);
            }
        });
    }
    w.cancel
        .on_clicked(with0(ide, |ide| ide.settings_window.window.hide()));
    w.ok.on_clicked(with0(ide, |ide| ide.settings_ok()));
    w.browse.on_clicked(with0(ide, |ide| {
        let w = &ide.settings_window;
        if let Some(dir) =
            dialog::choose_folder(Some(w.window.widget()), "Arbeitsverzeichnis wählen")
        {
            w.base_dir.set_text(&dir.to_string_lossy());
        }
    }));
    w.base_dir.on_text_changed(with(ide, |ide, text: String| {
        let w = &ide.settings_window;
        if !w.filling.get() {
            let text = text.trim();
            w.edit.borrow_mut().settings.base_dir = (!text.is_empty()).then(|| expand_home(text));
        }
    }));
    w.language
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            if let (false, Some(l)) = (
                w.filling.get(),
                usize::try_from(i).ok().and_then(|i| Language::ALL.get(i)),
            ) {
                w.edit.borrow_mut().settings.language = *l;
            }
        }));
    w.theme.on_current_index_changed(with(ide, |ide, i: i32| {
        let w = &ide.settings_window;
        if let (false, Some(t)) = (
            w.filling.get(),
            usize::try_from(i).ok().and_then(|i| Theme::ALL.get(i)),
        ) {
            w.edit.borrow_mut().settings.theme = *t;
        }
    }));
    w.method_order
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            if let (false, Some(order)) = (
                w.filling.get(),
                usize::try_from(i)
                    .ok()
                    .and_then(|i| MethodOrder::ALL.get(i)),
            ) {
                w.edit.borrow_mut().settings.method_order = *order;
            }
        }));
    w.profiles
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            if !w.filling.get() && i >= 0 {
                w.selected.set(i);
                w.fill_profile();
                w.update_state();
            }
        }));
    w.add
        .on_clicked(with0(ide, |ide| ide.settings_add_profile()));
    w.remove
        .on_clicked(with0(ide, |ide| ide.settings_remove_profile()));
    w.name.on_text_changed(with(ide, |ide, text: String| {
        let w = &ide.settings_window;
        if w.filling.get() {
            return;
        }
        let Some(p) = w.profile() else { return };
        // Nur gültige Namen übernehmen; sonst steht der Grund unter dem Key.
        let result = w.edit.borrow_mut().rename(&p.name, &text);
        match result {
            Ok(()) => {
                let i = w.selected.get();
                w.filling.set(true);
                w.profiles.set_item_text(i, text.trim());
                let names: Vec<String> = w
                    .edit
                    .borrow()
                    .settings
                    .ai
                    .profiles
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                for (combo, first) in [
                    (&w.default_profile, "(keins)"),
                    (&w.project_profile, "Standardprofil"),
                ] {
                    let current = combo.current_index();
                    let items: Vec<&str> = std::iter::once(first)
                        .chain(names.iter().map(String::as_str))
                        .collect();
                    combo.set_items(&items);
                    combo.set_current_index(current);
                }
                w.filling.set(false);
                w.update_state();
            }
            Err(e) if text.trim() != p.name => w
                .key_state
                .set_text(&format!("Name nicht übernommen: {}", e.detail)),
            Err(_) => {}
        }
    }));
    w.provider
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            if let Some(provider) = usize::try_from(i).ok().and_then(|i| AiProvider::ALL.get(i)) {
                w.change_profile(|p| p.provider = *provider);
                w.update_state();
            }
        }));
    w.model.on_text_changed(with(ide, |ide, text: String| {
        ide.settings_window
            .change_profile(|p| p.model = text.trim().to_owned());
    }));
    w.endpoint.on_text_changed(with(ide, |ide, text: String| {
        let w = &ide.settings_window;
        w.change_profile(|p| {
            let text = text.trim();
            p.endpoint = (!text.is_empty()).then(|| text.to_owned());
        });
    }));
    w.key.on_text_changed(with(ide, |ide, text: String| {
        let w = &ide.settings_window;
        if w.filling.get() {
            return;
        }
        if let Some(p) = w.profile() {
            let mut edit = w.edit.borrow_mut();
            if text.trim().is_empty() {
                edit.keys.remove(&p.name);
            } else {
                edit.keys.insert(p.name.clone(), Some(text));
            }
        }
        w.update_state();
    }));
    w.remove_key.on_clicked(with0(ide, |ide| {
        let w = &ide.settings_window;
        if let Some(p) = w.profile() {
            w.edit.borrow_mut().keys.insert(p.name.clone(), None);
            w.filling.set(true);
            w.key.set_text("");
            w.filling.set(false);
            w.update_state();
        }
    }));
    w.default_profile
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            if w.filling.get() {
                return;
            }
            let mut edit = w.edit.borrow_mut();
            let name = usize::try_from(i - 1)
                .ok()
                .and_then(|i| edit.settings.ai.profiles.get(i))
                .map(|p| p.name.clone());
            edit.settings.ai.default_profile = name;
        }));
    w.project_profile
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            let Some(project) = w.project.borrow().clone() else {
                return;
            };
            if w.filling.get() {
                return;
            }
            let mut edit = w.edit.borrow_mut();
            match usize::try_from(i - 1)
                .ok()
                .and_then(|i| edit.settings.ai.profiles.get(i))
                .map(|p| p.name.clone())
            {
                Some(name) => {
                    edit.settings.ai.projects.insert(project, name);
                }
                None => {
                    edit.settings.ai.projects.remove(&project);
                }
            }
        }));
    w.show_on_new.on_toggled(with(ide, |ide, on: bool| {
        let w = &ide.settings_window;
        if !w.filling.get() {
            w.edit.borrow_mut().settings.ai.show_on_new_project = on;
        }
    }));
    w.show_always.on_toggled(with(ide, |ide, on: bool| {
        let w = &ide.settings_window;
        if !w.filling.get() {
            w.edit.borrow_mut().settings.ai.show_always = on;
        }
    }));
}

/// `~/…` → home directory.
fn expand_home(text: &str) -> PathBuf {
    match text.strip_prefix("~") {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .map_or_else(
                || PathBuf::from(text),
                |home| home.join(rest.trim_start_matches(['/', '\\'])),
            ),
        _ => PathBuf::from(text),
    }
}
