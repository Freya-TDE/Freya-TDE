//! [`TypeRegistry`]: all component types known to a project – from the core,
//! the active UI backend and the loaded packs – plus backend-specific extra
//! properties for existing types.

use std::collections::BTreeMap;

use crate::component_type::ComponentType;
use crate::error::ModelError;
use crate::property::PropertyDef;
use crate::type_ref::TypeRef;

#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    types: BTreeMap<TypeRef, ComponentType>,
    /// (backend, type) → extra properties this backend adds to the type.
    extensions: BTreeMap<(String, TypeRef), Vec<PropertyDef>>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with the common widget core ([`crate::builtin`]).
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        for ty in crate::builtin::types() {
            registry.register(ty).expect("built-in types are unique");
        }
        registry
    }

    /// Registers a type; its [`TypeRef`] must be unique.
    pub fn register(&mut self, ty: ComponentType) -> Result<(), ModelError> {
        if self.types.contains_key(&ty.type_ref) {
            return Err(ModelError::DuplicateType(ty.type_ref));
        }
        self.types.insert(ty.type_ref.clone(), ty);
        Ok(())
    }

    /// Adds backend-specific properties to an existing type (e.g. the egui
    /// backend adds `sense` to `core/Button`). They apply only to projects
    /// using that backend.
    pub fn extend(
        &mut self,
        backend: impl Into<String>,
        type_ref: &TypeRef,
        properties: impl IntoIterator<Item = PropertyDef>,
    ) -> Result<(), ModelError> {
        if !self.types.contains_key(type_ref) {
            return Err(ModelError::UnknownType(type_ref.clone()));
        }
        let backend = backend.into();
        let entry = self
            .extensions
            .entry((backend.clone(), type_ref.clone()))
            .or_default();
        entry.extend(
            properties
                .into_iter()
                .map(|p| p.for_backend(backend.clone())),
        );
        Ok(())
    }

    pub fn get(&self, type_ref: &TypeRef) -> Option<&ComponentType> {
        self.types.get(type_ref)
    }

    pub fn types(&self) -> impl Iterator<Item = &ComponentType> {
        self.types.values()
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// All properties an instance of `type_ref` has under `backend`: the
    /// type's own (except those reserved for other backends) plus the
    /// backend's extensions.
    pub fn property_defs(&self, type_ref: &TypeRef, backend: &str) -> Vec<&PropertyDef> {
        let own = self
            .types
            .get(type_ref)
            .into_iter()
            .flat_map(|t| &t.properties)
            .filter(|p| p.backend.as_deref().is_none_or(|b| b == backend));
        let extra = self
            .extensions
            .get(&(backend.to_owned(), type_ref.clone()))
            .into_iter()
            .flatten();
        own.chain(extra).collect()
    }

    /// Looks up one property of `type_ref` under `backend`.
    pub fn property_def(
        &self,
        type_ref: &TypeRef,
        backend: &str,
        name: &str,
    ) -> Option<&PropertyDef> {
        self.property_defs(type_ref, backend)
            .into_iter()
            .find(|p| p.name == name)
    }

    /// The backend a property is reserved for, if it exists on `type_ref`
    /// only for another backend (for helpful messages).
    pub fn property_backend(&self, type_ref: &TypeRef, name: &str) -> Option<String> {
        let own = self
            .types
            .get(type_ref)
            .into_iter()
            .flat_map(|t| &t.properties);
        let extended = self
            .extensions
            .iter()
            .filter(|((_, t), _)| t == type_ref)
            .flat_map(|(_, props)| props);
        own.chain(extended)
            .filter(|p| p.name == name)
            .find_map(|p| p.backend.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::PropertyType;

    #[test]
    fn extensions_apply_per_backend() {
        let mut registry = TypeRegistry::with_builtins();
        let button = crate::builtin::button();
        registry
            .extend(
                "egui",
                &button,
                [PropertyDef::new("sense", PropertyType::Text, "click")],
            )
            .unwrap();
        assert!(registry.property_def(&button, "egui", "sense").is_some());
        assert!(registry.property_def(&button, "slint", "sense").is_none());
        assert_eq!(
            registry.property_backend(&button, "sense").as_deref(),
            Some("egui")
        );
        assert!(registry.property_def(&button, "slint", "caption").is_some());
        assert!(registry.extend("egui", &TypeRef::core("Nope"), []).is_err());
        assert!(
            registry
                .register(crate::builtin::types().remove(0))
                .is_err()
        );
    }
}
