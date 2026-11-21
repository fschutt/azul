//! The "Materials" section: azul's built-in material textures as backgrounds, lit by the app's
//! one light. "Light from the pointer" makes that light a point light in front of the screen at
//! the pointer: the leather's grain and the aluminium's sheen turn toward it as it moves, and the
//! aluminium, a metal, reflects the desktop behind the window. Off, the app's light is the
//! default one again.

use azul::{app::Light, prelude::*, widgets::*};

use crate::{captioned, keep, section};

/// The built-in materials (`builtin(<name>)`) and what the plate under each says.
const MATERIALS: [(&str, &str); 4] = [
    ("leather", "Leather"),
    ("bluejeans", "Blue jeans"),
    ("aluminium", "Aluminium - a metal: it reflects the desktop"),
    ("vellum", "Vellum"),
];

/// How far in front of the screen the pointer's light is, in points.
const POINTER_LIGHT_HEIGHT: f32 = 160.0;

/// The section's state. Lives in `Showcase::materials`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MaterialsDemo {
    /// Whether the app's light follows the pointer over the section.
    pointer_light: bool,
}

/// The section's card: the switch, then one plate per material.
pub(crate) fn materials_section(data: &RefAny, demo: &MaterialsDemo, theme: UiTheme) -> Dom {
    let mut plates =
        Dom::create_div().with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 12px;");
    for (name, label) in MATERIALS {
        plates.add_child(captioned(
            label,
            Dom::create_div().with_css(format!(
                "width: 180px; height: 64px; border-radius: 6px; background: builtin({name});"
            )),
        ));
    }
    let switch = CheckBox::create(demo.pointer_light)
        .with_accessibility_name("Light from the pointer")
        .with_on_toggle(data.clone(), on_pointer_light)
        .with_theme(theme)
        .dom();
    let card = section(
        "Materials",
        vec![captioned("Light from the pointer (a point light)", switch), plates],
    );
    if demo.pointer_light {
        // The light moves without a rebuild: the window draws its materials again.
        card.with_callback(
            EventFilter::Hover(HoverEventFilter::MouseOver),
            data.clone(),
            on_pointer_moved,
        )
    } else {
        card
    }
}

/// The switch: on, the light goes to the pointer at its next move; off, the default light.
extern "C" fn on_pointer_light(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    if !state.checked {
        info.set_app_light(Light::create());
    }
    keep(&mut data, |s| s.materials.pointer_light = state.checked)
}

/// The pointer moved over the section: the app's light is a point in front of it (desktop
/// points, as the light is placed).
extern "C" fn on_pointer_moved(_data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(at) = info.get_cursor_position_screen().into_option() {
        info.set_app_light(Light::point(at.x, at.y, POINTER_LIGHT_HEIGHT));
    }
    Update::DoNothing
}
