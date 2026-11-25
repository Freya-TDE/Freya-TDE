//! Window &tr!("settings.title") (File → Settings, Ctrl+,): tab &tr!("settings.tab_general")
//! (working directory, language, appearance) and tab "KI" (profiles, default
//! profile, profile and permission of the current project, AI window;
//! &tr!("settings.fetch") fetches the models of the shown profile, a key that would go
//! over `http://` to another machine is warned about). The window edits a
//! [`SettingsEdit`]; only "OK" stores keys, saves and applies
//! ([`Ide::update_settings`], also for automation).

use crate::tr;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use rncl::prelude::*;
use ruf::{Application, ColorScheme, dialog};

use super::{Ide, with, with0};
use crate::ai::{AiError, ModelInfo};
use crate::codetools::MethodOrder;
use crate::settings::{
    AiPermission, AiProfile, AiProvider, Language, SecretStore, Settings, SettingsEdit,
    SettingsError, Theme, codes,
};

const W: i32 = 660;
const H: i32 = 640;
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
    /// The KI page scrolls (its content is taller than small windows).
    ai_scroll: RnclScrollView,
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
    model: RnclComboBox,
    fetch: RnclButton,
    endpoint: RnclTextField,
    key: RnclTextField,
    key_state: RnclLabel,
    warning: RnclLabel,
    remove_key: RnclButton,
    default_profile: RnclComboBox,
    project_profile: RnclComboBox,
    permission: RnclComboBox,
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
    /// Number of the last &tr!("settings.fetch") (older answers are ignored).
    models_request: Cell<u64>,
    /// Result of &tr!("settings.fetch") (shown when there is no warning).
    models_notice: RefCell<String>,
}

/// Room for the vertical scroll bar of the KI page.
const SCROLLBAR: i32 = 16;

fn label<P: AsWidget + ?Sized>(parent: &P, text: &str) -> RnclLabel {
    RnclLabel::new(parent, text)
}

impl SettingsWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("settings.title"));
        window.set_size(W, H);
        let tabs = RnclTabView::new(&window);
        let general = tabs.add_tab(&tr!("settings.tab_general"));
        let ai = tabs.add_tab(&tr!("settings.tab_ai"));

        // Allgemein
        let p = &general;
        let general_labels = vec![
            label(p, &tr!("settings.work_dir")),
            label(p, &tr!("settings.language")),
            label(p, &tr!("settings.appearance")),
            label(p, &tr!("settings.new_methods")),
        ];
        let base_dir = RnclTextField::new(p);
        base_dir.set_placeholder(&tr!("settings.work_dir_placeholder"));
        let browse = RnclButton::new(p, "…");
        let base_hint = label(p, &tr!("settings.work_dir_hint"));
        let language = RnclComboBox::new(p);
        language.set_items(&Language::ALL.map(Language::label));
        let language_hint = label(p, &crate::tr!("settings.language_hint"));
        let theme = RnclComboBox::new(p);
        theme.set_items(&Theme::ALL.map(|t| match t {
            Theme::System => tr!("settings.theme_system"),
            other => other.label(),
        }));

        let method_order = RnclComboBox::new(p);
        method_order.set_items(&MethodOrder::ALL.map(MethodOrder::label));
        method_order
            .widget()
            .set_tool_tip(&tr!("settings.new_methods_tip"));

        // KI: in einer Bildlauffläche, damit nichts aus dem Fenster läuft.
        let ai_scroll = RnclScrollView::new(&ai);
        let p = ai_scroll.content();
        let profiles = RnclListBox::new(p);
        let add = RnclButton::new(p, &tr!("settings.new"));
        let remove = RnclButton::new(p, &tr!("settings.remove"));
        let ai_labels = vec![
            label(p, &tr!("settings.name")),
            label(p, &tr!("settings.provider")),
            label(p, &tr!("settings.model")),
            label(p, &tr!("settings.address")),
            label(p, &tr!("settings.api_key")),
            label(p, &tr!("settings.default_profile")),
            label(p, &tr!("settings.this_project")),
            label(p, &tr!("settings.ai_rights")),
        ];
        let name = RnclTextField::new(p);
        let provider = RnclComboBox::new(p);
        provider.set_items(&AiProvider::ALL.map(AiProvider::label));
        let model = RnclComboBox::new(p);
        model.set_editable(true);
        model.set_placeholder(&tr!("settings.model_placeholder"));
        let fetch = RnclButton::new(p, &tr!("settings.fetch"));
        fetch.widget().set_tool_tip(&tr!("settings.fetch_tip"));
        let endpoint = RnclTextField::new(p);
        let key = RnclTextField::new(p);
        key.set_password_mode(true);
        let key_state = label(p, "");
        let warning = label(p, "");
        warning.set_word_wrap(true);
        let remove_key = RnclButton::new(p, &tr!("settings.remove_key"));
        let default_profile = RnclComboBox::new(p);
        let project_profile = RnclComboBox::new(p);
        let permission = RnclComboBox::new(p);
        permission.set_items(&AiPermission::ALL.map(AiPermission::label));
        permission.widget().set_tool_tip(
            "Lesen ist immer erlaubt. „mit Vorschau“: jede Änderung erst als Vorschlag; \
             „selbstständig“: ohne Rückfrage",
        );
        let show_on_new = RnclCheckBox::new(p, &tr!("settings.show_ai_new"));
        let show_always = RnclCheckBox::new(p, &tr!("settings.show_ai_always"));
        let ai_hint = label(p, "");
        ai_hint.set_word_wrap(true);

        let cancel = RnclButton::new(&window, &tr!("settings.cancel"));
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
            fetch,
            endpoint,
            key,
            key_state,
            warning,
            remove_key,
            default_profile,
            project_profile,
            permission,
            show_on_new,
            show_always,
            ai_hint,
            ai_labels,
            ai_scroll,
            cancel,
            ok,
            edit: RefCell::new(SettingsEdit::default()),
            selected: Cell::new(-1),
            project: RefCell::new(None),
            filling: Cell::new(false),
            models_request: Cell::new(0),
            models_notice: RefCell::new(String::new()),
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

        // KI: links die Profile, rechts das gewählte Profil – alles im
        // Inhalt der Bildlauffläche (schmaler um die Bildlaufleiste).
        self.ai_scroll.set_geometry(0, 0, width, height);
        let inner = (width - 2 * p - SCROLLBAR).max(300);
        let fw = p + inner - fx;
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
        self.model.set_geometry(rfx, line(2), rfw - 90 - GAP, ROW);
        self.fetch.set_geometry(rfx + rfw - 90, line(2), 90, ROW);
        self.endpoint.set_geometry(rfx, line(3), rfw, ROW);
        let kw = rfw - 130 - GAP;
        self.key.set_geometry(rfx, line(4), kw, ROW);
        self.remove_key
            .set_geometry(rfx + kw + GAP, line(4), 130, ROW);
        self.key_state.set_geometry(rfx, line(5), rfw, ROW);
        self.warning.set_geometry(p, line(6), inner, ROW);
        for (i, l) in self.ai_labels.iter().skip(5).enumerate() {
            l.set_geometry(p, line(7 + i as i32), LABEL_W, ROW);
        }
        self.default_profile.set_geometry(fx, line(7), fw, ROW);
        self.project_profile.set_geometry(fx, line(8), fw, ROW);
        self.permission.set_geometry(fx, line(9), fw, ROW);
        self.show_on_new.set_geometry(p, line(10), inner, ROW);
        self.show_always.set_geometry(p, line(10) + ROW, inner, ROW);
        let y = line(10) + 2 * ROW + GAP;
        let hint_h = 3 * ROW;
        self.ai_hint.set_geometry(p, y, inner, hint_h);
        self.ai_scroll.set_content_size(0, y + hint_h + p);
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
        let mut choices = vec![tr!("settings.none")];
        choices.extend(names.iter().cloned());
        self.default_profile.set_items(&choices);
        let pos = |name: Option<&String>| {
            name.and_then(|n| names.iter().position(|x| x == n))
                .map_or(0, |i| i as i32 + 1)
        };
        self.default_profile
            .set_current_index(pos(s.ai.default_profile.as_ref()));
        choices[0] = tr!("settings.default_profile");
        self.project_profile.set_items(&choices);
        let project = self.project.borrow().clone();
        self.project_profile
            .set_current_index(pos(project.as_ref().and_then(|p| s.ai.projects.get(p))));
        let permission = s.ai.permission_for(project.as_deref());
        self.permission.set_current_index(
            AiPermission::ALL
                .iter()
                .position(|x| *x == permission)
                .unwrap_or(0) as i32,
        );
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
            self.fetch.widget(),
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
        self.model.clear();
        self.model.set_current_text(&p.model);
        self.models_notice.borrow_mut().clear();
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
                || tr!("settings.address_required"),
                |e| tr!("settings.address_empty", address = e),
            );
        self.endpoint.set_placeholder(&endpoint);
        let state = match &p {
            None => String::new(),
            Some(p) => match edit.keys.get(&p.name) {
                Some(Some(k)) if !k.trim().is_empty() => {
                    tr!("settings.key_new")
                }
                Some(_) => tr!("settings.key_removed"),
                None if p.key_in_keyring => tr!("settings.key_keyring"),
                None if p.key.is_some() => {
                    tr!("settings.key_file")
                }
                None if !p.provider.needs_key() => tr!("settings.key_not_needed"),
                None => tr!("settings.key_none"),
            },
        };
        self.key_state.set_text(&state);
        let warning = p.as_ref().and_then(|p| edit.key_warning(&p.name));
        self.warning.set_text(&match warning {
            Some(w) => format!("⚠ {}", w.detail),
            None => self.models_notice.borrow().clone(),
        });
        self.key.set_placeholder(&if p
            .as_ref()
            .is_some_and(|p| p.has_key() && !edit.keys.contains_key(&p.name))
        {
            tr!("settings.key_saved_placeholder")
        } else {
            tr!("settings.api_key")
        });
        let connected = edit.settings.ai.connected();
        self.show_on_new.widget().set_enabled(connected);
        self.show_always.widget().set_enabled(connected);
        self.default_profile.widget().set_enabled(connected);
        self.project_profile
            .widget()
            .set_enabled(connected && self.project.borrow().is_some());
        self.permission
            .widget()
            .set_enabled(connected && self.project.borrow().is_some());
        self.ai_hint.set_text(&if !connected {
            tr!("settings.hint_none")
        } else if self.project.borrow().is_none() {
            tr!("settings.hint_unsaved")
        } else {
            tr!("settings.hint_project")
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
        drop(settings);
        self.ai_window.refresh(self);
    }

    /// "OK" in the settings window.
    fn settings_ok(&self) {
        let edit = self.settings_window.edit.borrow().clone();
        match self.update_settings(&edit) {
            Ok(_) => {
                self.settings_window.window.hide();
                self.set_status(&tr!("settings.saved"));
            }
            Err(errors) => {
                let text: Vec<String> = errors
                    .iter()
                    .map(|e| format!("{} ({})", e.detail, e.code))
                    .collect();
                dialog::error(
                    Some(self.settings_window.window.widget()),
                    &tr!("settings.title"),
                    &text.join("\n"),
                );
            }
        }
    }

    /// &tr!("settings.new") on the AI tab: a profile with a free name.
    pub fn settings_add_profile(&self) {
        let w = &self.settings_window;
        {
            let mut edit = w.edit.borrow_mut();
            let profiles = &mut edit.settings.ai.profiles;
            let name = (1..)
                .map(|n| tr!("settings.profile_n", n = n))
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

    /// &tr!("settings.fetch") on the AI tab: fetches the models of the shown profile as
    /// edited (address and key before "OK") in a worker thread; the answer
    /// fills the model drop-down.
    pub fn settings_fetch_models(&self) {
        let w = &self.settings_window;
        let Some(p) = w.profile() else { return };
        let Some(sender) = self.ai_results.borrow().clone() else {
            return;
        };
        let client = w
            .edit
            .borrow()
            .client(&p.name, self.secrets.borrow().as_ref());
        let request = w.models_request.get() + 1;
        w.models_request.set(request);
        match client {
            Ok(client) => {
                *w.models_notice.borrow_mut() =
                    tr!("settings.asking_models", provider = p.provider.label());
                w.fetch.widget().set_enabled(false);
                std::thread::spawn(move || {
                    sender.send(super::ai_window::AiDone::Models(request, client.models()));
                });
            }
            Err(e) => *w.models_notice.borrow_mut() = format!("{} ({})", e.detail, e.code),
        }
        w.update_state();
    }

    /// The answer to &tr!("settings.fetch").
    pub(super) fn settings_models_done(
        &self,
        request: u64,
        result: Result<Vec<ModelInfo>, AiError>,
    ) {
        let w = &self.settings_window;
        if request != w.models_request.get() {
            return;
        }
        w.fetch.widget().set_enabled(w.profile().is_some());
        let notice = match result {
            Ok(models) if models.is_empty() => tr!("settings.no_models"),
            Ok(models) => {
                let current = w.model.current_text();
                let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
                let tips: Vec<&str> = models
                    .iter()
                    .map(|m| m.name.as_deref().unwrap_or(&m.id))
                    .collect();
                w.filling.set(true);
                w.model.set_items(&ids);
                w.model.set_item_tooltips(&tips);
                w.model.set_current_text(&current);
                w.filling.set(false);
                let n = models.len();
                let found = if n == 1 {
                    tr!("settings.one_model")
                } else {
                    tr!("settings.models_found", count = n)
                };
                if current.is_empty() {
                    tr!("settings.models_choose", found = found)
                } else {
                    format!("{found}.")
                }
            }
            Err(e) => tr!("settings.models_error", detail = e.detail, code = e.code),
        };
        *w.models_notice.borrow_mut() = notice;
        w.update_state();
    }

    /// The models offered in the drop-down of the settings window (after
    /// &tr!("settings.fetch")) and the status line under the key (tests).
    pub fn settings_models(&self) -> (Vec<String>, String) {
        let w = &self.settings_window;
        (w.model.items(), w.warning.text())
    }

    /// &tr!("settings.remove") on the AI tab.
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
            dialog::choose_folder(Some(w.window.widget()), &tr!("settings.choose_work_dir"))
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
                    (&w.default_profile, tr!("settings.none")),
                    (&w.project_profile, tr!("settings.default_profile")),
                ] {
                    let current = combo.current_index();
                    let items: Vec<&str> = std::iter::once(first.as_str())
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
                .set_text(&tr!("settings.name_rejected", detail = e.detail)),
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
    w.fetch
        .on_clicked(with0(ide, |ide| ide.settings_fetch_models()));
    w.permission
        .on_current_index_changed(with(ide, |ide, i: i32| {
            let w = &ide.settings_window;
            let Some(project) = w.project.borrow().clone() else {
                return;
            };
            let Some(permission) = usize::try_from(i)
                .ok()
                .and_then(|i| AiPermission::ALL.get(i))
            else {
                return;
            };
            if w.filling.get() {
                return;
            }
            let mut edit = w.edit.borrow_mut();
            if *permission == AiPermission::default() {
                edit.settings.ai.permissions.remove(&project);
            } else {
                edit.settings.ai.permissions.insert(project, *permission);
            }
        }));
    w.endpoint.on_text_changed(with(ide, |ide, text: String| {
        let w = &ide.settings_window;
        w.change_profile(|p| {
            let text = text.trim();
            p.endpoint = (!text.is_empty()).then(|| text.to_owned());
        });
        if !w.filling.get() {
            w.update_state();
        }
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
