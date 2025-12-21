use cliclack::{Theme, ThemeState};
use console::Style;

pub struct MagentaTheme;

impl Theme for MagentaTheme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => Style::new().blue(),
            ThemeState::Error(_) => Style::new().blue(),
            _ => Style::new().blue().dim()
        }
    }

    fn state_symbol_color(&self, _state: &ThemeState) -> Style {
        Style::new().blue()
    }

    fn info_symbol(&self) -> String {
        "⚙".into()
    }
}
