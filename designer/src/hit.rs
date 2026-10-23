//! What is under the mouse: elements and resize handles.

use serde::Serialize;
use tdr_core::ComponentName;

use crate::geometry::{Point, Rect};
use crate::layout::{FormLayout, ItemKind};

/// Edge length of a resize handle.
pub const HANDLE_SIZE: i32 = 7;

/// A resize handle of a selected widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

/// The eight handles around `r`.
pub fn handles(r: Rect) -> [(Handle, Rect); 8] {
    let s = HANDLE_SIZE;
    let at = |x: i32, y: i32| Rect::new(x - s / 2, y - s / 2, s, s);
    let (cx, cy) = (r.x + r.width / 2, r.y + r.height / 2);
    [
        (Handle::TopLeft, at(r.x, r.y)),
        (Handle::Top, at(cx, r.y)),
        (Handle::TopRight, at(r.right(), r.y)),
        (Handle::Right, at(r.right(), cy)),
        (Handle::BottomRight, at(r.right(), r.bottom())),
        (Handle::Bottom, at(cx, r.bottom())),
        (Handle::BottomLeft, at(r.x, r.bottom())),
        (Handle::Left, at(r.x, cy)),
    ]
}

/// The result of [`hit_test`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub element: ComponentName,
    /// Set if a resize handle of a selected widget was hit.
    pub handle: Option<Handle>,
}

/// The element at `point`: handles of selected widgets first, then the
/// innermost element.
pub fn hit_test(layout: &FormLayout, point: Point, selected: &[ComponentName]) -> Option<Hit> {
    for item in layout
        .items
        .iter()
        .filter(|i| i.kind != ItemKind::Component && selected.contains(&i.name))
    {
        if let Some((handle, _)) = handles(item.rect)
            .into_iter()
            .find(|(_, r)| r.contains(point))
        {
            return Some(Hit {
                element: item.name.clone(),
                handle: Some(handle),
            });
        }
    }
    // Later items are deeper (children follow their parents).
    layout
        .items
        .iter()
        .rev()
        .find(|i| i.rect.contains(point))
        .map(|i| Hit {
            element: i.name.clone(),
            handle: None,
        })
}
