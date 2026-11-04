//! Data binding in the backend tests: a data source without a database
//! server (fixed rows) in a data module with a Tokio runtime.

use tdr_core::data::{COLUMNS, DATA_SOURCE, DISPLAY_FIELD, VALUE_FIELD};
use tdr_core::pack::Pack;
use tdr_core::*;

/// A data source with fixed rows `(id, name)`.
const FAKE_DATA: &str = r#"
[pack]
name = "fakedata"
version = "0.1.0"
license = "MIT"
generated-code-license = "MIT"

[[component]]
name = "Rows"
requires = [{ capability = "tokio.runtime" }]
[component.codegen]
type = "Vec<(i64, String)>"
init = "vec![(1, \"Apfel\".to_string()), (2, \"Birne\".to_string()), (3, \"Kirsche\".to_string())]"
[component.data]
fetch = "source.clone()"
get = "{% if column == 'id' %}{{ row }}.0{% else %}{{ row }}.1.clone(){% endif %}"
"#;

pub fn register(registry: &mut TypeRegistry) {
    Pack::parse(FAKE_DATA).unwrap().register(registry).unwrap();
}

fn name(n: &str) -> ComponentName {
    ComponentName::new(n).unwrap()
}

/// Data module `services`: Tokio runtime and `fruits` (3 rows).
pub fn services(backend: &str) -> Form {
    let mut services = Form::data_module(name("services"), backend);
    services
        .add_component(Component::new(
            name("runtime"),
            TypeRef::pack("tokio", "Runtime"),
        ))
        .unwrap();
    services
        .add_component(
            Component::new(name("fruits"), TypeRef::pack("fakedata", "Rows")).with(
                COLUMNS,
                Value::List(vec![
                    Value::Text("id: i64".into()),
                    Value::Text("name: String".into()),
                ]),
            ),
        )
        .unwrap();
    services
}

/// `component` filled from `services::fruits`: shows `name`, value `id`.
pub fn bind(component: Component) -> Component {
    component
        .with(
            DATA_SOURCE,
            Value::reference_on(name("services"), name("fruits")),
        )
        .with(DISPLAY_FIELD, "name")
        .with(VALUE_FIELD, "id")
}
