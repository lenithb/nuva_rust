//! interfaz de nuva: modelo de vistas, navegación de teclado y tui.
//!
//! el modelo (`Tab`, `NavEvent`, `parse_key_event`) es agnóstico del
//! renderizador y lo comparten el modo cli, esta tui y la futura gui
//! (egui). navegación: ctrl+izquierda/derecha cambia de vista;
//! izquierda/derecha cambia de pista.

use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::crossterm::event;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

use crate::player::MusicPlayer;
use crate::playlist::{Queue, Repeat};

///vista principal de la app: pestañas sugeridos y cola.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    ///sugerencias online (fase d: radio-browser.info y archive.org).
    Sugeridos,
    ///biblioteca local en cola de reproducción.
    Playlists,
}

impl Tab {
    ///todas las pestañas, en orden de ciclo.
    pub const fn all() -> [Tab; 2] {
        [Tab::Sugeridos, Tab::Playlists]
    }

    ///pestaña siguiente (equivale a ctrl+derecha o `>`).
    pub fn next(self) -> Self {
        cycle(self, 1)
    }

    ///pestaña anterior (equivale a ctrl+izquierda o `<`).
    pub fn prev(self) -> Self {
        cycle(self, -1)
    }
}

fn cycle(from: Tab, step: i32) -> Tab {
    let all = Tab::all();
    let i = all.iter().position(|t| *t == from).unwrap_or(0) as i32;
    let len = all.len() as i32;
    all[((i + step).rem_euclid(len)) as usize]
}

///intervalo de sondeo del teclado en la tui.
const POLL: Duration = Duration::from_millis(120);
///justo después de `play()`, `is_finished()` puede devolver true un
///instante mientras la cola se prepara; ese margen se ignora para no
///saltar pistas.
const PLAY_GRACE: Duration = Duration::from_millis(500);

///ejecuta la tui sobre la cola dada. devuelve cuando el usuario sale.
pub fn run_tui(queue: Queue) -> Result<(), String> {
    // ratatui::init() habilita raw mode + pantalla alternativa y
    // devuelve el defaultterminal ya inicializado.
    let mut terminal = ratatui::init();
    let result = tui_loop(&mut terminal, queue);
    ratatui::restore();
    result
}

fn tui_loop(terminal: &mut ratatui::DefaultTerminal, mut queue: Queue) -> Result<(), String> {
    let player = &mut MusicPlayer::new()?;
    let mut list_state = ListState::default();
    let mut tab = Tab::Playlists;
    let mut sel = 0usize;
    let mut paused = false;
    let mut since_play = Instant::now();

    // bucle: sondeo el teclado y, al mismo tiempo, avanzo las pistas
    // automáticamente cuando terminan.
    loop {
        // auto-avance: la pista cargada terminó y el usuario no pausó.
        if !paused
            && player.current().is_some()
            && since_play.elapsed() > PLAY_GRACE
            && player.is_finished()
        {
            match queue.next() {
                Some(path) => {
                    play_now(player, path);
                    since_play = Instant::now();
                }
                None => player.stop(),
            }
        }

        // eventos de teclado.
        if event::poll(POLL).map_err(|e| e.to_string())? {
            if let event::Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind == event::KeyEventKind::Press {
                    use event::KeyCode::*;
                    match (key.code, key.modifiers) {
                        (Char('q') | Esc, _) => break,
                        (Char('c'), event::KeyModifiers::CONTROL) => break, // ctrl+c
                        (Right, event::KeyModifiers::CONTROL) => tab = tab.next(),
                        (Left, event::KeyModifiers::CONTROL) => tab = tab.prev(),
                        (Right, _) => {
                            track_step(player, &mut queue, &mut paused, true, &mut since_play)
                        }
                        (Left, _) => {
                            track_step(player, &mut queue, &mut paused, false, &mut since_play)
                        }
                        (Char(' '), _) => {
                            player.toggle();
                            paused = player.is_paused();
                        }
                        (Up, _) | (Char('k'), _) => sel = sel.saturating_sub(1),
                        (Down, _) | (Char('j'), _) => {
                            if sel + 1 < queue.len() {
                                sel += 1;
                            }
                        }
                        (Enter, _) => {
                            queue.select(sel);
                            if let Some(path) = queue.current() {
                                paused = false;
                                play_now(player, path);
                                since_play = Instant::now();
                            }
                        }
                        (Char('a'), _) => queue.toggle_shuffle(),
                        (Char('r'), _) => {
                            let next = match queue.repeat() {
                                Repeat::Off => Repeat::All,
                                Repeat::All => Repeat::One,
                                Repeat::One => Repeat::Off,
                            };
                            queue.set_repeat(next);
                        }
                        (Char('+') | Char('='), _) => player.set_volume(player.volume() + 0.05),
                        (Char('-'), _) => player.set_volume(player.volume() - 0.05),
                        (Char('s'), _) => player.stop(),
                        _ => {}
                    }
                }
            }
        }

        // dibujo
        terminal
            .draw(|frame| draw(frame, player, &queue, tab, &mut list_state, sel))
            .map_err(|e| e.to_string())?;
    }

    player.stop();
    Ok(())
}

///reproduce una pista; si falla, se ignora el error por ahora
///(la futura gui mostrará un aviso en pantalla).
fn play_now(player: &mut MusicPlayer, path: &Path) {
    let _ = player.play(path);
}

///cambia de pista usando la cola (flechas ←/→).
fn track_step(
    player: &mut MusicPlayer,
    queue: &mut Queue,
    paused: &mut bool,
    forward: bool,
    since_play: &mut Instant,
) {
    let track = if forward { queue.next() } else { queue.prev() };
    if let Some(path) = track {
        *paused = false;
        play_now(player, path);
        *since_play = Instant::now();
    }
}

///logo del encabezado. por hoy: texto sin asset.
///
///cuando exista el recurso gráfico (fase de branding): svg maestro en
///`assets/logo.svg` y png derivado en `assets/logo.png` para la tui
///(render con ratatui-image), con fallback a este texto en
///terminales sin soporte de gráficos.
fn header_logo() -> Span<'static> {
    Span::styled("nuva", Style::default().fg(Color::Magenta))
}

///dibuja el frame completo: header (pestañas), cuerpo (vista) y barra
///inferior de estado (now playing).
fn draw(
    frame: &mut Frame,
    player: &MusicPlayer,
    queue: &Queue,
    tab: Tab,
    list_state: &mut ListState,
    sel: usize,
) {
    let [header, body, status] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(4),
    ])
    .areas(frame.area());

    // ── header ───────────────────────────────────────────────────
    let tabs = Tab::all()
        .iter()
        .map(|t| {
            let name = match t {
                Tab::Sugeridos => " [Sugeridos] ".to_owned(),
                Tab::Playlists => " [Cola] ".to_owned(),
            };
            if *t == tab {
                Span::styled(name, Style::default().bg(Color::Magenta).fg(Color::Black))
            } else {
                Span::styled(name, Style::default().fg(Color::DarkGray))
            }
        })
        .collect::<Line>()
        .centered();

    let title = Line::from(header_logo()).centered();
    frame.render_widget(Paragraph::new(vec![title, tabs]), header);

    // ── cuerpo según pestaña ─────────────────────────────────────
    match tab {
        Tab::Playlists => {
            let items: Vec<_> = queue.iter().map(|p| ListItem::new(file_name(p))).collect();
            list_state.select(Some(sel.clamp(0, queue.len().saturating_sub(1))));
            let list = List::new(items)
                .block(
                    Block::bordered()
                        .title(Span::styled(
                            " Cola — Enter reproduce · j/k navega · Ctrl+flechas vistas ",
                            Style::default().fg(Color::Magenta),
                        ))
                        .border_style(Style::default().fg(Color::DarkGray)),
                )
                .highlight_symbol("▶ ")
                .highlight_style(Style::default().bg(Color::Indexed(234)).fg(Color::Magenta));
            frame.render_stateful_widget(list, body, list_state);
        }
        Tab::Sugeridos => sugeridos(frame, body),
    }

    // ── barra inferior (now playing) ─────────────────────────────
    let [gauge_area, footer] =
        Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(status);

    let now = player
        .current()
        .map(crate::playlist::file_name)
        .unwrap_or_else(|| "nada en reproducción".into());
    let state = if player.current().is_none() {
        "⏹"
    } else if player.is_paused() {
        "⏸"
    } else {
        "▶"
    };
    let rep = match queue.repeat() {
        Repeat::Off => String::new(),
        Repeat::All => " · rep:todas".into(),
        Repeat::One => " · rep:una".into(),
    };
    let shuf = if queue.shuffled() { " · aleat" } else { "" };
    let info_line = Line::from(vec![
        Span::styled(format!("{state} {now}"), Style::default().fg(Color::White)),
        Span::styled(
            format!("  — vol {}%{rep}{shuf}", (player.volume() * 100.0).round()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let dur = player.duration().unwrap_or(Duration::from_secs(1));
    let pos = player.position();
    let pct = if dur.is_zero() {
        0
    } else {
        (pos.as_secs_f64() / dur.as_secs_f64() * 100.0).clamp(0.0, 100.0) as u16
    };
    let gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_style(Style::default().fg(Color::DarkGray))
                .title(info_line),
        )
        .gauge_style(Style::default().fg(Color::Magenta))
        .label(format!(
            "{} / {}",
            format_hms(pos),
            format_hms(player.duration().unwrap_or_default())
        ))
        .percent(pct);
    frame.render_widget(gauge, gauge_area);

    // pie con los atajos principales.
    let ayuda = Line::from(Span::styled(
        "→/← pista · espacio pausa · a aleatorio · r repetir · +/- volumen · s detener · q salir",
        Style::default().fg(Color::DarkGray),
    ))
    .centered();
    frame.render_widget(Paragraph::new(ayuda), footer);
}

///placeholder de sugeridos hasta conectar la api (fase d).
fn sugeridos(frame: &mut Frame, area: ratatui::layout::Rect) {
    let block = Block::bordered()
        .padding(Padding::horizontal(2))
        .title(Span::styled(
            " Sugeridos ",
            Style::default().fg(Color::Magenta),
        ))
        .border_style(Style::default().fg(Color::DarkGray));
    let plain = Style::default().fg(Color::Gray);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Muy pronto: sugerencias en vivo.",
                plain,
            )),
            Line::default(),
            Line::from(Span::styled(
                "· radio-browser.info — radios sin clave API (streaming directo)",
                plain,
            )),
            Line::from(Span::styled(
                "· archive.org — música CC / dominio público a la carta",
                plain,
            )),
            Line::from(Span::styled(
                "· MusicBrainz — metadatos y carátulas sin API key",
                plain,
            )),
            Line::default(),
            Line::from(Span::styled(
                "Ctrl+→ para ver tu Cola mientras tanto",
                Style::default().fg(Color::Magenta),
            )),
        ])
        .block(block),
        area,
    );
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

///formato mm:ss (u h:mm:ss si supera una hora).
fn format_hms(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

///eventos de alto nivel que emite la navegación de teclado.
///
///el cli los detecta vía `parse_key_event`; la tui los mapea
///directamente de crossterm. la gui (egui) seguirá este modelo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavEvent {
    ///ctrl+right (o ctrl+">"): vista siguiente.
    NextTab,
    ///ctrl+left (o ctrl+"<"): vista anterior.
    PrevTab,
    ///right: pista siguiente.
    NextTrack,
    ///left: pista anterior.
    PrevTrack,
}

///traduce una línea cruda del terminal (modo cli) a un evento de
///navegación. en modo canónico las secuencias llegan como líneas:
///`ESC[1;5C` = ctrl+right, `ESC[1;5D` = ctrl+left,
///`ESC[C` = right, `ESC[D` = left.
pub fn parse_key_event(line: &str) -> Option<NavEvent> {
    match line {
        "\u{1b}[1;5C" | "\u{1b}OC" => Some(NavEvent::NextTab),
        "\u{1b}[1;5D" | "\u{1b}OD" => Some(NavEvent::PrevTab),
        "\u{1b}[C" => Some(NavEvent::NextTrack),
        "\u{1b}[D" => Some(NavEvent::PrevTrack),
        _ => None,
    }
}
