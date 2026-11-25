//! Window "Datenbank" (schema browser) and the database actions of the IDE:
//! test a pool's connection, take a query's columns from the database,
//! show tables, views, columns, functions and procedures of all pools.
//!
//! Requests go to the helper `freya-tde-db` (module [`crate::database`]).
//! The buttons run them in the background; the `database_*` methods of
//! [`Ide`] run them directly (automation, tests, AI).

use crate::tr;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rncl::prelude::*;
use ruf::Color;
use tdr_core::ComponentPath;
use tdr_core::schema::{
    Connection, DbError, Described, Driver, Keys, RoutineKind, Schema, TableKind,
};
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
    /// Load the schema (SQL) or the keys (Redis) for the window.
    Schema,
}

/// The answer of a request.
#[derive(Debug, Clone)]
pub enum DbAnswer {
    Server(String),
    Described(Described),
    Schema(Schema),
    Keys(Keys),
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
            DbJob::Schema if connection.driver == Driver::Redis => db
                .keys(connection, "*", database::KEY_LIMIT)
                .map(DbAnswer::Keys),
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
            DbJob::Columns if !database::is_redis_read(forms, path) => {
                Some(database::query_sql(forms, path)?)
            }
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

    /// The schema of the SQL pool `path` (also kept for the window).
    pub fn database_schema(&self, path: &ComponentPath) -> Result<Schema, DbError> {
        let result = self.database_run(DbJob::Schema, path);
        self.db_schemas
            .borrow_mut()
            .insert(path.clone(), result.clone());
        self.database_window.refresh(self);
        match result? {
            DbAnswer::Schema(schema) => Ok(schema),
            _ => Err(DbError::new(
                tdr_core::schema::codes::UNSUPPORTED,
                "Redis has keys, not tables: use database_keys",
            )),
        }
    }

    /// Keys of the Redis client `path` (or of a Redis read's client) matching
    /// `pattern`, at most `limit`.
    pub fn database_keys(
        &self,
        path: &ComponentPath,
        pattern: &str,
        limit: usize,
    ) -> Result<Keys, DbError> {
        let (connection, _) = self.database_target(DbJob::Test, path)?;
        let mut guard = self.db_access.0.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.database_dir();
        if guard.as_ref().is_none_or(|d| d.dir() != dir) {
            *guard = Some(Database::new(dir));
        }
        guard
            .as_mut()
            .expect("set above")
            .keys(&connection, pattern, limit)
    }

    /// Preview of "take columns from the database" for query `path`: the
    /// command that would be applied and what the database reported.
    pub fn query_columns_preview(
        &self,
        path: &ComponentPath,
    ) -> Result<(Command, Described), DbError> {
        let redis = database::is_redis_read(self.session().document().forms(), path);
        let described = if redis {
            // Redis: die Spalten folgen aus der Art, kein Server nötig.
            let session = self.session();
            database::redis_columns(
                session.document().forms(),
                path,
                session.document().registry(),
            )?
        } else {
            match self.database_run(DbJob::Columns, path)? {
                DbAnswer::Described(d) => d,
                _ => unreachable!("describe answers with columns"),
            }
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
        let Some(sender) = self.db_results.borrow().clone() else {
            return;
        };
        if job == DbJob::Columns && database::is_redis_read(self.session().document().forms(), path)
        {
            let result = self
                .query_columns_preview(path)
                .map(|(_, d)| DbAnswer::Described(d));
            self.db_busy.set(self.db_busy.get() + 1);
            sender.send((job, path.clone(), result));
            return;
        }
        let target = self.database_target(job, path);
        let (connection, sql) = match target {
            Ok(t) => t,
            Err(error) => {
                sender.send((job, path.clone(), Err(error)));
                return;
            }
        };
        let what = tr!(match job {
            DbJob::Test => "db.testing",
            DbJob::Columns => "db.reading_columns",
            DbJob::Schema => "db.loading_schema",
        });
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
                tr!(
                    "db.error",
                    path = path,
                    code = error.code,
                    detail = error.detail
                )
            }
            (_, Ok(DbAnswer::Server(server))) => tr!("db.connected", path = path, server = server),
            (_, Ok(DbAnswer::Described(described))) => {
                let command =
                    database::columns_command(self.session().document().forms(), &path, &described);
                match command {
                    Ok(command) => {
                        self.apply(command);
                        format!("{path}: {}", database::columns_summary(&described))
                    }
                    Err(error) => {
                        tr!(
                            "db.error",
                            path = path,
                            code = error.code,
                            detail = error.detail
                        )
                    }
                }
            }
            (_, Ok(DbAnswer::Schema(schema))) => {
                let text = tr!(
                    "db.schema_summary",
                    path = path,
                    server = schema.server,
                    tables = schema.tables.len(),
                    routines = schema.routines.len()
                );
                self.db_schemas
                    .borrow_mut()
                    .insert(path.clone(), Ok(DbAnswer::Schema(schema)));
                text
            }
            (_, Ok(DbAnswer::Keys(keys))) => {
                let more = if keys.truncated { "+" } else { "" };
                let text = tr!(
                    "db.keys_summary",
                    path = path,
                    server = keys.server,
                    count = format!("{}{more}", keys.keys.len())
                );
                self.db_schemas
                    .borrow_mut()
                    .insert(path.clone(), Ok(DbAnswer::Keys(keys)));
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
    schemas: &BTreeMap<ComponentPath, Result<DbAnswer, DbError>>,
) -> Vec<TreeItem> {
    pools
        .iter()
        .map(|(path, connection)| {
            let id = format!("pool:{path}");
            let label = match connection {
                Some(c) => format!(
                    "{path} ({}, {})",
                    c.driver.name(),
                    database::display_url(&c.url)
                ),
                None => tr!("db.no_url", path = path),
            };
            let mut item = TreeItem::new(&id, &label);
            match schemas.get(path) {
                None => {
                    item =
                        item.with_child(TreeItem::new(&format!("{id}:wait"), &tr!("db.loading")));
                }
                Some(Ok(DbAnswer::Server(_) | DbAnswer::Described(_))) => {}
                Some(Err(error)) => {
                    item = item.with_child(
                        TreeItem::new(
                            &format!("{id}:error"),
                            &tr!("db.tree_error", code = error.code, detail = error.detail),
                        )
                        .with_color(Color::rgb(0xc0, 0x39, 0x2b)),
                    );
                }
                Some(Ok(DbAnswer::Keys(keys))) => {
                    item = item.with_child(TreeItem::new(&format!("{id}:server"), &keys.server));
                    let count = if keys.truncated {
                        format!("{}+", keys.keys.len())
                    } else {
                        keys.keys.len().to_string()
                    };
                    let mut group = TreeItem::new(&format!("{id}:keys"), &tr!("inspector.db.keys"))
                        .with_badge(&count);
                    for key in &keys.keys {
                        let key_id = format!("key:{path}:{}", key.key);
                        let mut node = TreeItem::new(&key_id, &key.key)
                            .with_badge(&key.kind)
                            .collapsed();
                        let mut facts = Vec::new();
                        if let Some(length) = key.length {
                            facts.push(tr!("db.length", length = length));
                        }
                        facts.push(match key.ttl {
                            Some(ttl) => tr!("db.expires", seconds = ttl),
                            None => tr!("db.no_expiry"),
                        });
                        node = node.with_child(TreeItem::new(
                            &format!("{key_id}:facts"),
                            &facts.join(", "),
                        ));
                        for (i, value) in key.preview.iter().enumerate() {
                            node = node.with_child(TreeItem::new(&format!("{key_id}:{i}"), value));
                        }
                        group = group.with_child(node);
                    }
                    if keys.truncated {
                        group = group.with_child(TreeItem::new(
                            &format!("{id}:more"),
                            &tr!("db.more_keys", count = database::KEY_LIMIT),
                        ));
                    }
                    item = item.with_child(group);
                }
                Some(Ok(DbAnswer::Schema(schema))) => {
                    item = item.with_child(TreeItem::new(&format!("{id}:server"), &schema.server));
                    for (kind, title) in [
                        (TableKind::Table, tr!("db.tables")),
                        (TableKind::View, tr!("db.views")),
                    ] {
                        let title = title.as_str();
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
                            (RoutineKind::Function, "functions"),
                            (RoutineKind::Procedure, "procedures"),
                        ]
                    };
                    for &(kind, key) in routine_groups {
                        let title = tr!(&format!("db.{key}"));
                        let title = title.as_str();
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
    /// Grip and content (dockable, see `dock`).
    pub pane: super::dock::DockPane,
    reload: RnclButton,
    test: RnclButton,
    hint: RnclLabel,
    tree: RnclTreeView,
}

const BAR: i32 = 36;

impl DatabaseWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("menu.window.database"));
        let pane = super::dock::DockPane::with_body(
            &window,
            crate::desktop::window::DATABASE,
            &tr!("menu.window.database"),
        );
        let body = pane.body().clone();
        let reload = RnclButton::new(&body, &tr!("menu.git.refresh"));
        let test = RnclButton::new(&body, &tr!("db.test_connection"));
        let hint = RnclLabel::new(&body, "");
        hint.set_word_wrap(true);
        let tree = RnclTreeView::new(&body);
        Self {
            window,
            pane,
            reload,
            test,
            hint,
            tree,
        }
    }

    /// Buttons on top, the hint (if any) wrapped in its own line below them,
    /// then the tree.
    fn layout(&self, w: i32, h: i32) {
        self.reload.set_geometry(6, 4, 120, BAR - 8);
        self.test.set_geometry(132, 4, 150, BAR - 8);
        let hint_h = if self.hint.text().is_empty() {
            0
        } else {
            // Zwei Zeilen reichen auch schmal (Umbruch).
            2 * 20 + 4
        };
        self.hint.set_geometry(6, BAR, (w - 12).max(0), hint_h);
        let top = BAR + hint_h;
        self.tree.set_geometry(0, top, w, (h - top).max(0));
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
            tr!("db.none_hint")
        } else if ide.db_busy.get() > 0 {
            tr!("db.busy")
        } else {
            String::new()
        };
        if self.hint.text() != hint {
            self.hint.set_text(&hint);
            let size = self.pane.body().size();
            self.layout(size.width, size.height);
        }
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
            .or_else(|| id.strip_prefix("routine:"))
            .or_else(|| id.strip_prefix("key:"))?;
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
    window.pane.body().widget().on_resized(move |w, h| {
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
            None => ide.set_status(&tr!("db.none")),
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
