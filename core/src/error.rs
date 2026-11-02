//! Errors of model operations (as opposed to validation findings, which are
//! [`crate::Diagnostic`]s).

use crate::name::ComponentName;
use crate::type_ref::TypeRef;

/// Serialized as `{"code": "duplicate_name", "detail": …}`; the codes are
/// stable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ModelError {
    #[error("invalid {kind} {text:?}: must be a Rust identifier and not a keyword")]
    InvalidIdentifier { kind: &'static str, text: String },
    #[error("invalid type reference {0:?}: expected core/Name, backend:id/Name or pack:id/Name")]
    InvalidTypeRef(String),
    #[error(
        "invalid capability {0:?}: expected lowercase segments separated by dots, e.g. sqlx.pool"
    )]
    InvalidCapability(String),
    #[error("type {0} is registered twice")]
    DuplicateType(TypeRef),
    #[error("backend {0:?} is registered twice")]
    DuplicateBackend(String),
    #[error("unknown type {0}")]
    UnknownType(TypeRef),
    #[error("type {type_ref} already has a property {property:?}")]
    DuplicateProperty { type_ref: TypeRef, property: String },
    #[error("name {0} is already used on this form")]
    DuplicateName(ComponentName),
    #[error("{0} not found on this form")]
    NotFound(ComponentName),
    #[error("{0} cannot be moved into its own subtree")]
    MoveIntoItself(ComponentName),
    #[error("the root widget of a form cannot be removed")]
    CannotRemoveRoot,
    #[error("form file format {0} is newer than this version of Freya TDE supports")]
    UnsupportedFormat(u32),
    #[error("invalid package name {0:?}: letters, digits, `-` and `_`, starting with a letter")]
    InvalidPackageName(String),
    #[error("RON: {0}")]
    Ron(String),
}
