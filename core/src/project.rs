//! Editing operations across all forms of a project.
//!
//! References may point to other forms (`services::db_pool`), so renaming or
//! removing a component – or renaming a form – must update every form, not
//! just the one the component lives on.

use crate::error::ModelError;
use crate::form::Form;
use crate::name::ComponentName;

fn find_form<'a>(forms: &'a mut [Form], name: &ComponentName) -> Result<&'a mut Form, ModelError> {
    forms
        .iter_mut()
        .find(|f| &f.name == name)
        .ok_or_else(|| ModelError::NotFound(name.clone()))
}

/// Renames `form::old` to `new` and updates references on every form.
pub fn rename_component(
    forms: &mut [Form],
    form: &ComponentName,
    old: &ComponentName,
    new: ComponentName,
) -> Result<(), ModelError> {
    find_form(forms, form)?.rename(old, new.clone())?;
    for other in forms.iter_mut().filter(|f| &f.name != form) {
        let current = other.name.clone();
        other.for_each_component_mut(|c| c.rename_references(&current, form, old, &new));
    }
    Ok(())
}

/// Removes `form::name` (a widget with its subtree) and clears references to
/// the removed elements on every form. Returns the removed names.
pub fn remove_component(
    forms: &mut [Form],
    form: &ComponentName,
    name: &ComponentName,
) -> Result<Vec<ComponentName>, ModelError> {
    let removed = find_form(forms, form)?.remove(name)?;
    for other in forms.iter_mut().filter(|f| &f.name != form) {
        let current = other.name.clone();
        other.for_each_component_mut(|c| {
            for gone in &removed {
                c.clear_references(&current, form, gone);
            }
        });
    }
    Ok(removed)
}

/// Renames a form and updates qualified references (`old::x` → `new::x`)
/// on every form.
pub fn rename_form(
    forms: &mut [Form],
    old: &ComponentName,
    new: ComponentName,
) -> Result<(), ModelError> {
    if forms.iter().any(|f| f.name == new) {
        return Err(ModelError::DuplicateName(new));
    }
    find_form(forms, old)?.name = new.clone();
    for form in forms.iter_mut() {
        form.for_each_component_mut(|c| c.rename_form_references(old, &new));
    }
    Ok(())
}

/// Removes a form and clears references to its elements on every other
/// form. Returns the removed form.
pub fn remove_form(forms: &mut Vec<Form>, name: &ComponentName) -> Result<Form, ModelError> {
    let index = forms
        .iter()
        .position(|f| &f.name == name)
        .ok_or_else(|| ModelError::NotFound(name.clone()))?;
    let removed = forms.remove(index);
    let gone: Vec<ComponentName> = removed
        .elements()
        .iter()
        .map(|e| e.name().clone())
        .collect();
    for other in forms.iter_mut() {
        let current = other.name.clone();
        other.for_each_component_mut(|c| {
            for element in &gone {
                c.clear_references(&current, name, element);
            }
        });
    }
    Ok(removed)
}
