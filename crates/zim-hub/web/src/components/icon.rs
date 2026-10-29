//! Inline SVG icons. Text glyphs (☰, ⓘ, ⧉ …) render as a missing-glyph
//! box on iOS, whose system fonts lack them; strokes in `currentColor`
//! follow the surrounding text colour and theme.

use yew::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Info,
    Copy,
    Chevron,
    Upload,
    Back,
}

impl Icon {
    /// The SVG's inner shapes on a 16×16 grid.
    fn shapes(self) -> Html {
        match self {
            Icon::Info => html! {
                <>
                    <circle cx="8" cy="8" r="6" />
                    <path d="M8 7.2V11" />
                    <path d="M8 4.9v.1" />
                </>
            },
            Icon::Copy => html! {
                <>
                    <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
                    <path d="M10.5 3.5v-.5a1 1 0 0 0-1-1h-6.5a1 1 0 0 0-1 1v6.5a1 1 0 0 0 1 1h.5" />
                </>
            },
            Icon::Chevron => html! { <path d="M4 6l4 4 4-4" /> },
            Icon::Upload => html! { <path d="M8 13V3M4 7l4-4 4 4" /> },
            Icon::Back => html! { <path d="M13 8H3M7 4L3 8l4 4" /> },
        }
    }
}

/// Render `icon` at 1em. `label` becomes the accessible name; without
/// one the icon is decorative (hidden from assistive tech).
pub fn icon(icon: Icon, label: Option<&str>) -> Html {
    html! {
        <svg class="icon" viewBox="0 0 16 16" width="1em" height="1em" fill="none"
            stroke="currentColor" stroke-width="1.5" stroke-linecap="round"
            stroke-linejoin="round"
            aria-label={label.map(str::to_string)}
            aria-hidden={label.is_none().then_some("true")}>
            { icon.shapes() }
        </svg>
    }
}
