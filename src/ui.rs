//! Modelo de vistas y navegación de nuva.
//!
//! Define las pestañas y los eventos de teclado de forma
//! agnóstica del renderizador: la TUI (ratatui) y la GUI (egui)
//! los consumirán igual. El CLI actual los emite ya con teclas
//! reales (Ctrl+←/→) traducidas por `parse_key_event`.

/// Pestañas principales de la app.
///
/// El orden define el ciclo: Sugeridos ↔ Playlists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// Vista principal con sugerencias (fase D: API gratuita de música).
    Sugeridos,
    /// Vista de biblioteca local en cola.
    Playlists,
}

impl Tab {
    pub const fn all() -> [Tab; 2] {
        [Tab::Sugeridos, Tab::Playlists]
    }

    /// Siguiente pestaña (equivale a Ctrl+Right / `>`).
    pub fn next(self) -> Self {
        let all = Self::all();
        let i = all.iter().position(|t| *t == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }

    /// Pestaña anterior (equivale a Ctrl+Left / `<`).
    pub fn prev(self) -> Self {
        let all = Self::all();
        let i = all.iter().position(|t| *t == self).unwrap_or(0);
        all[(i + all.len() - 1) % all.len()]
    }
}

/// Eventos de alto nivel que emite la navegación de teclado.
///
/// El CLI los detecta hoy; ratatui/egui los detectarán mañana con
/// las mismas constantes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavEvent {
    /// Ctrl+Right (o Ctrl+">"): vista siguiente.
    NextTab,
    /// Ctrl+Left (o Ctrl+"<"): vista anterior.
    PrevTab,
    /// Right: pista siguiente.
    NextTrack,
    /// Left: pista anterior.
    PrevTrack,
}

/// Traduce una línea cruda del terminal a un evento de navegación.
///
/// En modo canónico las secuencias del teclado llegan como líneas:
///   `ESC[1;5C` = Ctrl+Right, `ESC[1;5D` = Ctrl+Left,
///   `ESC[C` = Right, `ESC[D` = Left.
pub fn parse_key_event(line: &str) -> Option<NavEvent> {
    match line {
        "\u{1b}[1;5C" | "\u{1b}OC" => Some(NavEvent::NextTab),
        "\u{1b}[1;5D" | "\u{1b}OD" => Some(NavEvent::PrevTab),
        "\u{1b}[C" => Some(NavEvent::NextTrack),
        "\u{1b}[D" => Some(NavEvent::PrevTrack),
        _ => None,
    }
}
