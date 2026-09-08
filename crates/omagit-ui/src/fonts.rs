//! Resolving the typography stacks of DESIGN-TOKENS §8.
//!
//! The contract names *stacks* — `system-ui, -apple-system, 'Inter', sans-serif`
//! — the way CSS does, where the browser walks the list and takes the first
//! family it has. The renderer's `font_family` takes a single family and has no
//! such walk: hand it a family the machine lacks and the text falls back to
//! whatever the platform picks, which is neither of the two stacks.
//!
//! So the walk happens here, once, at start-up: each stack is resolved against
//! the installed families and the winner is cached. A Linux box without Inter
//! and a macOS box without JetBrains Mono then both land on a sane family
//! instead of an arbitrary one.
//!
//! `system-ui` and `-apple-system` are generics, not families: the platform
//! knows its own UI face under a private name the font list never enumerates
//! (`.SystemUIFont` on macOS). The caller passes that name in — resolving it
//! here would need a `cfg(target_os)` in the UI layer, which SPEC §3 rule 6
//! forbids.

use gpui_kit::{App, Global, SharedString};

use omagit_theme::font;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fonts {
    pub ui: SharedString,
    pub mono: SharedString,
}

impl Global for Fonts {}

/// Resolve both stacks against what the machine actually has.
///
/// `system_ui` is the platform's own UI family, from
/// `Platform::system_ui_family`.
pub fn resolve(system_ui: &str, cx: &mut App) {
    let available = cx.text_system().all_font_names();
    let fonts = Fonts {
        ui: first_available(font::UI, &available, system_ui),
        mono: first_available(font::MONO, &available, system_ui),
    };
    tracing::info!(ui = %fonts.ui, mono = %fonts.mono, "resolved font stacks");
    cx.set_global(fonts);
}

fn first_available(stack: &[&str], available: &[String], system_ui: &str) -> SharedString {
    for family in stack {
        match *family {
            // A generic standing for the platform's UI face. It is never in the
            // font list, and it always resolves, so the walk stops here.
            "system-ui" | "-apple-system" => return system_ui.into(),
            _ => {
                if available
                    .iter()
                    .any(|installed| installed.eq_ignore_ascii_case(family))
                {
                    return (*family).into();
                }
            }
        }
    }
    // The last entry of each stack is a generic family (`sans-serif`,
    // `monospace`) that no machine lists by name but every platform
    // understands, so it is the right thing to hand over unresolved.
    stack.last().copied().unwrap_or("sans-serif").into()
}

pub trait ActiveFonts {
    fn fonts(&self) -> &Fonts;
}

impl ActiveFonts for App {
    fn fonts(&self) -> &Fonts {
        self.global::<Fonts>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    const SYSTEM_UI: &str = ".SystemUIFont";

    #[test]
    fn the_ui_stack_stops_at_the_platform_face() {
        // `system-ui` heads the stack, so the platform's own face wins even
        // where Inter is installed — which is what the design expects, since
        // each platform's UI metrics differ (DESIGN.md, mock-up 02).
        let available = vec!["Inter".to_owned()];
        assert_eq!(first_available(font::UI, &available, SYSTEM_UI), SYSTEM_UI);
    }

    #[test]
    fn the_mono_stack_takes_the_first_installed_family() {
        let available = vec!["JetBrains Mono".to_owned()];
        assert_eq!(
            first_available(font::MONO, &available, SYSTEM_UI),
            "JetBrains Mono"
        );
    }

    #[test]
    fn matching_ignores_case() {
        let available = vec!["jetbrains mono".to_owned()];
        assert_eq!(
            first_available(font::MONO, &available, SYSTEM_UI),
            "JetBrains Mono"
        );
    }

    #[test]
    fn falls_back_to_the_generic_family() {
        // A machine without JetBrains Mono still gets a usable stack end, not
        // the first entry it cannot resolve.
        let available: Vec<String> = Vec::new();
        assert_eq!(
            first_available(font::MONO, &available, SYSTEM_UI),
            "monospace"
        );
    }
}
