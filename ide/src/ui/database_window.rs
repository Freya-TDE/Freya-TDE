//! Window "Datenbank" (schema browser) and the database actions of the IDE:
//! test a pool's connection, take a query's columns from the database,
//! show tables, views, columns, functions and procedures of all pools.
//!
//! Requests go to the helper `freya-tde-db` (module [`crate::database`]).
//! The buttons run them in the background; the `database_*` methods of
//! [`Ide`] run them directly (automation, tests, AI).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rncl::prelude::*;
use ruf::Color;
use tdr_core::ComponentPath;
use tdr_core::schema::{Connection, DbError, Described, RoutineKind, Schema, TableKind};
use tdr_designer::Command;

use super::{Ide, with, with0};
use crate::database::{self, Database};
use crate::desktop::window;
use crate::output::channel;

/// What a background request does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbJob {
    /// Connect to the pool (or the query's pool).
    Test,
    /// Read the result columns of a query into `columns`.
    Columns,
    /// Load the schema for the window.
    Schema,
}

/// The answer of a request.
#[derive(Debug, Clone)]
pub enum DbAnswer {
    Server(String),
    Described(Described),
    Schema(Schema),
}

/// Delivered to the GUI thread: (job, component, answer).
pub type DbDone = (DbJob, ComponentPath, Result<DbAnswer, DbError>);

/// The helper for one project folder, shared with background requests.
#[derive(Default, Clone)]
pub struct DbAccess(Arc<Mutex<Option<Database>>>);

impl DbAccess {
    /// Runs `job` with the helper of `dir` (started on demand, restarted if
    /// the folder changed).
    fn run(
        &self,
        dir: PathBuf,
        job: DbJob,
        connection: &Connection,
        sql: Option<&str>,
    ) -> Result<DbAnswer, DbError> {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_none_or(|d| d.dir() != dir) {
            *guard = Some(Database::new(dir));
        }
        let db = guard.as_mut().expect("set above");
        match job {
            DbJob::Test => db.test(connection).map(DbAnswer::Server),
            DbJob::Schema => db.schema(connection).map(DbAnswer::Schema),
            DbJob::Columns => db
                .describe(connection, sql.unwrap_or_default())
                .map(DbAnswer::Described),
        }
    }
}

impl Ide {
    /// The folder the helper runs in: the project folder, else the current
    /// folder (unsaved project).
    fn database_dir(&self) -> PathBuf {
        self.session()
            .project_dir()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }

    /// Connection and (for [`DbJob::Columns`]) SQL of `path`.
    fn database_target(
        &self,
        job: DbJob,
        path: &ComponentPath,
    ) -> Result<(Connection, Option<String>), DbError> {
        let session = self.session();
        let forms = session.document().forms();
        let connection = database::connection(forms, path)?;
        let sql = match job {
            DbJob::Columns => Some(database::query_sql(forms, path)?),
            _ => None,
        };
        Ok((connection, sql))
    }

    fn database_run(&self, job: DbJob, path: &ComponentPath) -> Result<DbAnswer, DbError> {
        let (connection, sql) = self.database_target(job, path)?;
        self.db_access
            .run(self.database_dir(), job, &connection, sql.as_deref())
    }

    /// Connects to the pool `path` (or the pool of query `path`) and returns
    /// the server version.
    pub fn database_test(&self, path: &ComponentPath) -> Result<String, DbError> {
        match self.database_run(DbJob::Test, path)? {
            DbAnswer::Server(server) => Ok(server),
            _ => unreachable!("test answers with the server"),
        }
    }

    /// The schema of the pool `path` (also kept for the window).
    pub fn database_schema(&self, path: &ComponentPath) -> Result<Schema, DbError> {
        let result = match self.database_run(DbJob::Schema, path) {
            Ok(DbAnswer::Schema(schema)) => Ok(schema),
            Ok(_) => unreachable!("schema answers with a schema"),
            Err(e) => Err(e),
        };
        self.db_schemas
            .borrow_mut()
            .insert(path.clone(), result.clone());
        self.database_window.refresh(self);
        result
    }

    /// Preview of "take columns from the database" for query `path`: the
    /// command that would be applied and what the database reported.
    pub fn query_columns_preview(
        &self,
        path: &ComponentPath,
    ) -> Result<(Command, Described), DbError> {
        let described = match self.database_run(DbJob::Columns, path)? {
            DbAnswer::Described(d) => d,
            _ => unreachable!("describe answers with columns"),
        };
        let command =
            database::columns_command(self.session().document().forms(), path, &described)?;
        Ok((command, described))
    }

    /// Takes the result columns of query `path` into its `columns` property
    /// (undoable).
    pub fn take_query_columns(&self, path: &ComponentPath) -> Result<Described, DbError> {
        let (command, described) = self.query_columns_preview(path)?;
        self.apply(command);
        Ok(described)
    }

    /// Runs `job` for `path` in the background; the result appears in the
    /// status line, the output channel "Datenbank" and the window.
    pub fn database_job(&self, job: DbJob, path: &ComponentPath) {
        let target = self.database_target(job, path);
        let Some(sender) = self.db_results.borrow().clone() else {
            return;
        };
        let (connection, sql) = match target {
            Ok(t) => t,
            Err(error) => {
                sender.send((job, path.clone(), Err(error)));
                return;
            }
        };
        let what = match job {
            DbJob::Test => "Verbindung wird getestet",
            DbJob::Columns => "Spalten werden gelesen",
            DbJob::Schema => "Schema wird geladen",
        };
        self.set_status(&format!("{path}: {what} …"));
        self.db_busy.set(self.db_busy.get() + 1);
        let access = self.db_access.clone();
        let dir = self.database_dir();
        let path = path.clone();
        std::thread::spawn(move || {
            let result = access.run(dir, job, &connection, sql.as_deref());
            sender.send((job, path, result));
        });
    }

    /// A background request is done.
    pub(super) fn database_finished(&self, (job, path, result): DbDone) {
        self.db_busy.set(self.db_busy.get().saturating_sub(1));
        let message = match (&job, result) {
            (_, Err(error)) => {
                if job == DbJob::Schema {
                    self.db_schemas
                        .borrow_mut()
                        .insert(path.clone(), Err(error.clone()));
                }
                format!("{path}: Fehler {} – {}", error.code, error.detail)
            }
            (_, Ok(DbAnswer::Server(server))) => format!("{path}: verbunden mit {server}"),
            (_, Ok(DbAnswer::Described(described))) => {
                let command =
                    database::columns_command(self.session().document().forms(), &path, &described);
                match command {
                    Ok(command) => {
                        self.apply(command);
                        format!("{path}: {}", database::columns_summary(&described))
                    }
                    Err(error) => format!("{path}: Fehler {} – {}", error.code, error.detail),
                }
            }
            (_, Ok(DbAnswer::Schema(schema))) => {
                let text = format!(
                    "{path}: {} – {} Tabellen/Sichten, {} Routinen",
                    schema.server,
                    schema.tables.len(),
                    schema.routines.len()
                );
                self.db_schemas
                    .borrow_mut()
                    .insert(path.clone(), Ok(schema));
                text
            }
        };
        self.output
            .append(channel::DATABASE, &format!("{message}\n"));
        self.set_status(&message);
        self.database_window.refresh(self);
    }

    /// Shows the window and loads the schemas of all pools.
    pub fn show_database(&self) {
        self.show_window(window::DATABASE);
        self.database_reload();
    }

    /// Loads the schemas of all pools again.
    pub fn database_reload(&self) {
        let pools = database::pools(self.session().document().forms());
        for pool in &pools {
            self.database_job(DbJob::Schema, pool);
        }
        self.database_window.refresh(self);
    }

    /// The tree the window shows (automation).
    pub fn database_tree(&self) -> Vec<TreeItem> {
        self.database_window.tree.items()
    }
}

/// Node ids: `pool:<path>`, `table:<path>:<schema>.<name>`, others below.
fn tree(
    pools: &[(ComponentPath, Option<Connection>)],
    schemas: &BTreeMap<ComponentPath, Result<Schema, DbError>>,
) -> Vec<TreeItem> {
    pools
        .iter()
        .map(|(path, connection)| {
            let id = format!("pool:{path}");
            let label = match connection {
                Some(c) => format!("{path} ({}, {})", c.driver.name(), c.url),
                None => format!("{path} (ohne URL)"),
            };
            let mut item = TreeItem::new(&id, &label);
            match schemas.get(path) {
                None => {
                    item = item.with_child(TreeItem::new(&format!("{id}:wait"), "… wird geladen"));
                }
                Some(Err(error)) => {
                    item = item.with_child(
                        TreeItem::new(
                            &format!("{id}:error"),
                            &format!("Fehler {}: {}", error.code, error.detail),
                        )
                        .with_color(Color::rgb(0xc0, 0x39, 0x2b)),
                    );
                }
                Some(Ok(schema)) => {
                    item = item.with_child(TreeItem::new(&format!("{id}:server"), &schema.server));
                    for (kind, title) in
                        [(TableKind::Table, "Tabellen"), (TableKind::View, "Sichten")]
                    {
                        let tables: Vec<_> =
                            schema.tables.iter().filter(|t| t.kind == kind).collect();
                        let mut group = TreeItem::new(&format!("{id}:{title}"), title)
                            .with_badge(&tables.len().to_string());
                        for table in tables {
                            let table_id = format!("table:{path}:{}.{}", table.schema, table.name);
                            let mut node = TreeItem::new(&table_id, &table.name)
                                .with_badge(&table.schema)
                                .collapsed();
                            for column in &table.columns {
                                let rust = column.rust_type.as_deref().unwrap_or("?");
                                let key = if column.primary_key { "🔑 " } else { "" };
                                let mut leaf = TreeItem::new(
                                    &format!("{table_id}:{}", column.name),
                                    &format!("{key}{}: {rust}", column.name),
                                )
                                .with_badge(&column.sql_type);
                                if column.rust_type.is_none() {
                                    leaf = leaf.with_color(Color::rgb(0xb0, 0x60, 0x00));
                                }
                                node = node.with_child(leaf);
                            }
                            group = group.with_child(node);
                        }
                        item = item.with_child(group.collapsed());
                    }
                    // SQLite kennt keine gespeicherten Routinen.
                    let routine_groups: &[(RoutineKind, &str)] = if connection
                        .as_ref()
                        .is_some_and(|c| c.driver == tdr_core::schema::Driver::Sqlite)
                    {
                        &[]
                    } else {
                        &[
                            (RoutineKind::Function, "Funktionen"),
                            (RoutineKind::Procedure, "Prozeduren"),
                        ]
                    };
                    for &(kind, title) in routine_groups {
                        let routines: Vec<_> =
                            schema.routines.iter().filter(|r| r.kind == kind).collect();
                        let mut group = TreeItem::new(&format!("{id}:{title}"), title)
                            .with_badge(&routines.len().to_string());
                        for r in routines {
                            let returns = r
                                .returns
                                .as_ref()
                                .map(|t| format!(" → {t}"))
                                .unwrap_or_default();
                            group = group.with_child(
                                TreeItem::new(
                                    &format!("routine:{path}:{}.{}", r.schema, r.name),
                                    &format!("{}({}){returns}", r.name, r.arguments),
                                )
                                .with_badge(&r.schema),
                            );
                        }
                        item = item.with_child(group.collapsed());
                    }
                }
            }
            item
        })
        .collect()
}

/// Keeps the expansion of nodes that exist before and after a rebuild.
fn keep_expansion(new: &mut [TreeItem], old: &[TreeItem]) {
    fn find<'a>(items: &'a [TreeItem], id: &str) -> Option<&'a TreeItem> {
        items
            .iter()
            .find_map(|i| (i.id == id).then_some(i).or_else(|| find(&i.children, id)))
    }
    for item in new.iter_mut() {
        if let Some(previous) = find(old, &item.id) {
            item.expanded = previous.expanded;
        }
        keep_expansion(&mut item.children, old);
    }
}

pub struct DatabaseWindow {
    pub window: RnclWindow,
    reload: RnclButton,
    test: RnclButton,
    hint: RnclLabel,
    tree: RnclTreeView,
}

const BAR: i32 = 36;

impl DatabaseWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new("Datenbank");
        let reload = RnclButton::new(&window, "Aktualisieren");
        let test = RnclButton::new(&window, "Verbindung testen");
        let hint = RnclLabel::new(&window, "");
        let tree = RnclTreeView::new(&window);
        Self {
            window,
            reload,
            test,
            hint,
            tree,
        }
    }

    fn layout(&self, w: i32, h: i32) {
        self.reload.set_geometry(6, 4, 120, BAR - 8);
        self.test.set_geometry(132, 4, 150, BAR - 8);
        self.hint.set_geometry(290, 4, (w - 296).max(0), BAR - 8);
        self.tree.set_geometry(0, BAR, w, (h - BAR).max(0));
    }

    pub fn refresh(&self, ide: &Ide) {
        let session = ide.session();
        let forms = session.document().forms();
        let pools: Vec<(ComponentPath, Option<Connection>)> = database::pools(forms)
            .into_iter()
            .map(|p| {
                let c = database::connection(forms, &p).ok();
                (p, c)
            })
            .collect();
        drop(session);
        let hint = if pools.is_empty() {
            "Keine Datenbank im Projekt – einen SQLx-Pool aus der Palette ablegen."
        } else if ide.db_busy.get() > 0 {
            "Lädt …"
        } else {
            ""
        };
        self.hint.set_text(hint);
        let mut items = tree(&pools, &ide.db_schemas.borrow());
        keep_expansion(&mut items, &self.tree.items());
        if items != self.tree.items() {
            self.tree.set_items(items);
        }
    }

    /// The pool of the selected node.
    fn selected_pool(&self) -> Option<ComponentPath> {
        let id = self.tree.selected()?;
        let rest = id
            .strip_prefix("pool:")
            .or_else(|| id.strip_prefix("table:"))
            .or_else(|| id.strip_prefix("routine:"))?;
        // `form::name` – danach folgt höchstens `:…`.
        let mut parts = rest.splitn(4, ':');
        let form = parts.next()?;
        let _ = parts.next();
        let name = parts.next()?;
        format!("{form}::{name}").parse().ok()
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let window = &ide.database_window;
    let weak = Rc::downgrade(ide);
    window.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.database_window.layout(w, h);
        }
    });
    window
        .reload
        .on_clicked(with0(ide, |ide| ide.database_reload()));
    window.test.on_clicked(with0(ide, |ide| {
        let pools = database::pools(ide.session().document().forms());
        let target = ide
            .database_window
            .selected_pool()
            .or_else(|| pools.first().cloned());
        match target {
            Some(pool) => ide.database_job(DbJob::Test, &pool),
            None => ide.set_status("Keine Datenbank im Projekt."),
        }
    }));
    // Doppelklick auf einen Pool wählt ihn im Designer.
    window.tree.on_item_activated(with(ide, |ide, id: String| {
        let Some(rest) = id.strip_prefix("pool:") else {
            return;
        };
        let Ok(path) = rest.parse::<ComponentPath>() else {
            return;
        };
        if let Some(form) = &path.form {
            if ide.session().form_name() != Some(form) {
                ide.select_form(form);
            }
            ide.select(vec![path.name.clone()]);
            ide.show_window(window::INSPECTOR);
        }
    }));
}
