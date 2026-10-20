//! interfaz de nuva: modelo de vistas, navegación de teclado y tui.
//!
//! el modelo (`Tab`, `NavEvent`, `parse_key_event`) es agnóstico del
//! renderizador y lo comparten el modo cli, esta tui y la futura gui
//! (egui). navegación: ctrl+izquierda/derecha cambia de vista;
//! izquierda/derecha cambia de pista.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use ratatui::crossterm::event;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::online::{self, Estacion, Tema as TemaOnline};
use crate::player::MusicPlayer;
use crate::playlist::{Queue, Repeat};

/// logo de marca de agua: png binario (ya recortado y con el trazo
/// ensanchado al generarlo) listo para pintarlo con bloques de medio paso.
struct Logo {
    brillos: image::GrayImage,
}

impl Logo {
    /// carga el asset una sola vez; si falla, la tui queda sin marca.
    fn cargar() -> Option<Self> {
        let img = image::load_from_memory(include_bytes!("../assets/logo.png"))
            .ok()?
            .to_luma8();
        Some(Self { brillos: img })
    }

    /// tamaño del dibujo.
    fn tamano(&self) -> (u32, u32) {
        (self.brillos.width(), self.brillos.height())
    }
}

/// marca de agua del cuerpo: el logo centrado en la zona con la tinta
/// en el tono atenuado del tema; las listas y el gauge dibujan encima.
fn fondo_logo(frame: &mut Frame, zona: Rect, logo: &Logo, pal: &Paleta) {
    let (ancho, alto) = logo.tamano();
    if zona.width < 20 || zona.height < 4 || ancho == 0 || alto == 0 {
        return;
    }

    // encaja el aspecto del logo en la zona (una celda = 2 pixeles de
    // alto) limitando el ancho a 2/3 para que no sature la vista.
    let ratio = ancho as f64 / alto as f64;
    let cols = (zona.width as u32 * 2 / 3).max(20).min(zona.width as u32);
    let filas = (cols as f64 / (2.0 * ratio)).floor() as u16;
    let filas = filas.clamp(1, zona.height);
    let dest = Rect::new(
        zona.x + (zona.width - cols as u16) / 2,
        zona.y + (zona.height - filas) / 2,
        cols as u16,
        filas,
    );

    // lanczos y dilatacion: sin ellos el trazo fino se esfuma.
    let mut escala = image::imageops::resize(
        &logo.brillos,
        dest.width as u32,
        dest.height as u32 * 2,
        image::imageops::FilterType::Lanczos3,
    );
    dilata(&escala.clone(), &mut escala, 1);

    for fila in 0..dest.height {
        let mut spans = Vec::with_capacity(dest.width as usize);
        for col in 0..dest.width as u32 {
            let lums = (
                escala[(col, fila as u32 * 2)][0],
                escala[(col, fila as u32 * 2 + 1)][0],
            );
            // tinta en tono atenuado: sutil bajo las listas y legible
            // en ambos temas (gris oscuro sobre negro y sobre blanco).
            let tinta = if lums.0 > 127 { pal.atenuado } else { pal.fondo };
            let suelo = if lums.1 > 127 { pal.atenuado } else { pal.fondo };
            spans.push(Span::styled("▀", Style::default().fg(tinta).bg(suelo)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(dest.x, dest.y + fila, dest.width, 1),
        );
    }
}

/// max de la vecindad en un radio dado (ensancha el trazo).
fn dilata(src: &image::GrayImage, dst: &mut image::GrayImage, radio: u32) {
    let (w, h) = (src.width(), src.height());
    for y in 0..h {
        for x in 0..w {
            let mut v = 0u8;
            for dy in -(radio as i64)..=(radio as i64) {
                for dx in -(radio as i64)..=(radio as i64) {
                    let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                    if xx >= 0 && yy >= 0 && (xx as u32) < w && (yy as u32) < h {
                        v = v.max(src[(xx as u32, yy as u32)][0]);
                    }
                }
            }
            dst[(x, y)] = image::Luma([v]);
        }
    }
}

/// mensaje que regresa del hilo de búsqueda online.
enum SugBuf {
    Radios(Vec<Estacion>),
    Temas(Vec<TemaOnline>),
    Error(String),
}

/// temas de la tui, fijos: ultra oscuro (por defecto) y claro.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tema {
    Oscuro,
    Claro,
}

impl Tema {
    /// alterna entre oscuro y claro.
    fn alterna(self) -> Self {
        if self == Tema::Oscuro {
            Tema::Claro
        } else {
            Tema::Oscuro
        }
    }
}

/// paleta monocroma del tema: fondo #000000/#ffffff y su contrario.
#[derive(Clone, Copy)]
struct Paleta {
    fondo: Color,
    texto: Color,
    /// tono secundario (bordes, metadatos, atajos).
    atenuado: Color,
}

impl Tema {
    fn paleta(self) -> Paleta {
        match self {
            // ultra oscuro: negro puro con blanco.
            Tema::Oscuro => Paleta {
                fondo: Color::Rgb(0, 0, 0),
                texto: Color::Rgb(255, 255, 255),
                atenuado: Color::DarkGray,
            },
            // claro: blanco puro con negro.
            Tema::Claro => Paleta {
                fondo: Color::Rgb(255, 255, 255),
                texto: Color::Rgb(0, 0, 0),
                atenuado: Color::DarkGray,
            },
        }
    }
}

/// fuente de la vista sugeridos.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FuenteSug {
    Radios,
    Temas,
}

impl FuenteSug {
    /// altera entre radios y temas.
    fn alterna(self) -> Self {
        if self == FuenteSug::Radios {
            FuenteSug::Temas
        } else {
            FuenteSug::Radios
        }
    }

    /// consulta por defecto al activar cada fuente.
    fn consulta_default(self) -> &'static str {
        if self == FuenteSug::Radios {
            "lofi"
        } else {
            "blues"
        }
    }

    /// rótulo legible de la fuente.
    fn texto(self) -> &'static str {
        if self == FuenteSug::Radios {
            "radios"
        } else {
            "temas cc"
        }
    }
}

/// estado completo de la vista sugeridos: fuente activa, listados,
/// búsqueda en curso y último error.
struct SugEstado {
    fuente: FuenteSug,
    consulta: &'static str,
    radios: Vec<Estacion>,
    temas: Vec<TemaOnline>,
    rx: Option<Receiver<SugBuf>>,
    err: Option<String>,
}

impl SugEstado {
    /// estado inicial: radios sobre la consulta por defecto.
    fn nueva() -> Self {
        Self {
            fuente: FuenteSug::Radios,
            consulta: "lofi",
            radios: Vec::new(),
            temas: Vec::new(),
            rx: None,
            err: None,
        }
    }

    /// selector del listado activo (para limitar j/k).
    fn total(&self) -> usize {
        match self.fuente {
            FuenteSug::Radios => self.radios.len(),
            FuenteSug::Temas => self.temas.len(),
        }
    }

    /// botón de la lista marcado como stream para conectar.
    fn eleccion(&self, sel: usize) -> Option<(String, String)> {
        match self.fuente {
            FuenteSug::Radios => self
                .radios
                .get(sel)
                .map(|e| (e.nombre.clone(), e.url.clone())),
            FuenteSug::Temas => self
                .temas
                .get(sel)
                .and_then(|t| t.url.clone().map(|u| (t.titulo.clone(), u))),
        }
    }

    /// recibe (sin bloquear) el resultado de la búsqueda pendiente.
    fn recibe(&mut self) {
        let Some(rx) = &self.rx else { return };
        match rx.try_recv() {
            Ok(SugBuf::Radios(rs)) => {
                self.radios = rs;
                self.rx = None;
            }
            Ok(SugBuf::Temas(ts)) => {
                self.temas = ts;
                self.rx = None;
            }
            Ok(SugBuf::Error(e)) => {
                self.err = Some(e);
                self.rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.rx = None,
        }
    }

    /// lanza la búsqueda de la fuente activa en un hilo aparte.
    fn buscar(&mut self) {
        lanzar_busqueda(self.fuente, self.consulta, &mut self.rx, &mut self.err);
    }

    /// alterna la fuente y reconsulta con el término por defecto.
    fn alterna_fuente(&mut self) {
        self.fuente = self.fuente.alterna();
        self.consulta = self.fuente.consulta_default();
        self.buscar();
    }
}

/// vista principal de la app: pestañas sugeridos y cola.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// sugerencias online (fase d: radio-browser.info y archive.org).
    Sugeridos,
    /// biblioteca local en cola de reproducción.
    Playlists,
}

impl Tab {
    /// todas las pestañas, en orden de ciclo.
    pub const fn all() -> [Tab; 2] {
        [Tab::Sugeridos, Tab::Playlists]
    }

    /// pestaña siguiente (equivale a ctrl+derecha o `>`).
    pub fn next(self) -> Self {
        cycle(self, 1)
    }

    /// pestaña anterior (equivale a ctrl+izquierda o `<`).
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

/// intervalo de sondeo del teclado en la tui.
const POLL: Duration = Duration::from_millis(120);
/// justo después de `play()`, `is_finished()` puede devolver true un
/// instante mientras la cola se prepara; ese margen se ignora para no
/// saltar pistas.
const PLAY_GRACE: Duration = Duration::from_millis(500);

/// ejecuta la tui sobre la cola dada. devuelve cuando el usuario sale.
pub fn run_tui(queue: Queue) -> Result<(), String> {
    // ratatui::init() habilita raw mode + pantalla alternativa y
    // devuelve el defaultterminal ya inicializado.
    let mut terminal = ratatui::init();
    let logo = Logo::cargar();
    let result = tui_loop(&mut terminal, queue, logo.as_ref());
    ratatui::restore();
    result
}

fn tui_loop(
    terminal: &mut ratatui::DefaultTerminal,
    mut queue: Queue,
    logo: Option<&Logo>,
) -> Result<(), String> {
    let player = &mut MusicPlayer::new()?;
    let mut list_state = ListState::default();
    let mut tab = Tab::Sugeridos;
    let mut sel = 0usize;
    let mut paused = false;
    let mut since_play = Instant::now();

    // estado de la vista sugeridos (fase d); la primera entrada a la
    // vista dispara la búsqueda inicial.
    let mut sug = SugEstado::nueva();
    sug.buscar();
    let mut tema = Tema::Oscuro;

    // bucle: sondeo el teclado y, al mismo tiempo, avanzo las pistas
    // automáticamente cuando terminan.
    loop {
        // recibo del hilo online (si hay búsqueda en curso).
        sug.recibe();

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
                        (Right, event::KeyModifiers::CONTROL) => {
                            tab = tab.next();
                            sel = 0;
                            list_state.select(Some(0));
                        }
                        (Left, event::KeyModifiers::CONTROL) => {
                            tab = tab.prev();
                            sel = 0;
                            list_state.select(Some(0));
                        }
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
                            if sel + 1 < sug.total() {
                                sel += 1;
                            }
                        }
                        (Char('a'), _) => queue.toggle_shuffle(),
                        (Char('t'), _) => tema = tema.alterna(),
                        (Char('f'), _) => {
                            sug.alterna_fuente();
                            sel = 0;
                        }
                        (Enter, _) if tab == self::Tab::Sugeridos => {
                            // conecta la sugerencia marcada: radio (stream
                            // continuo) o tema (mp3 remoto ya resuelto).
                            if let Some((nombre, url)) = sug.eleccion(sel) {
                                paused = false;
                                play_stream_now(player, &url, &nombre);
                                since_play = Instant::now();
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
            .draw(|frame| {
                draw(frame, player, &queue, tab, &mut list_state, sel, &sug, tema, logo)
            })
            .map_err(|e| e.to_string())?;
    }

    player.stop();
    Ok(())
}

/// reproduce una pista; si falla, se ignora el error por ahora
/// (la futura gui mostrará un aviso en pantalla).
fn play_now(player: &mut MusicPlayer, path: &Path) {
    let _ = player.play(path);
}

/// cambia de pista usando la cola (flechas ←/→).
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

/// conecta un stream online (radio o tema remoto).
fn play_stream_now(player: &mut MusicPlayer, url: &str, nombre: &str) {
    let _ = player.play_stream(url, nombre);
}

/// lanza la búsqueda online en un hilo aparte para no bloquear la tui;
/// el resultado llega por canal y el bucle lo recibe sin bloquear.
fn lanzar_busqueda(
    fuente: FuenteSug,
    consulta: &str,
    sug_rx: &mut Option<Receiver<SugBuf>>,
    sug_err: &mut Option<String>,
) {
    if sug_rx.is_some() {
        return; // ya hay una búsqueda en curso
    }
    *sug_err = None;
    let (tx, rx) = mpsc::channel();
    let q = consulta.to_owned();
    match fuente {
        FuenteSug::Radios => {
            std::thread::spawn(move || {
                let r = online::buscar_radios(&q, 24)
                    .map(SugBuf::Radios)
                    .unwrap_or_else(SugBuf::Error);
                let _ = tx.send(r);
            });
        }
        FuenteSug::Temas => {
            std::thread::spawn(move || {
                let r = online::buscar_temas(&q, 12)
                    .and_then(|mut ts| {
                        for t in &mut ts {
                            t.url = Some(online::resolver_tema(&t.id)?);
                        }
                        Ok(ts)
                    })
                    .map(SugBuf::Temas)
                    .unwrap_or_else(SugBuf::Error);
                let _ = tx.send(r);
            });
        }
    }
    *sug_rx = Some(rx);
}

/// dibuja el frame completo: header (pestañas), cuerpo (vista) y barra
/// inferior de estado (now playing).
#[allow(clippy::too_many_arguments)]
fn draw(
    frame: &mut Frame,
    player: &MusicPlayer,
    queue: &Queue,
    tab: Tab,
    list_state: &mut ListState,
    sel: usize,
    sug: &SugEstado,
    tema: Tema,
    logo: Option<&Logo>,
) {
    let pal = tema.paleta();

    // fondo del tema en toda la pantalla (el resto dibuja encima).
    frame.render_widget(Block::new().style(Style::default().bg(pal.fondo)), frame.area());

    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
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
                Span::styled(name, Style::default().bg(pal.texto).fg(pal.fondo))
            } else {
                Span::styled(name, Style::default().fg(pal.atenuado))
            }
        })
        .collect::<Line>()
        .centered();

    frame.render_widget(Paragraph::new(tabs), header);

    // marca de agua: zona del cuerpo y de reproducción, bajo las
    // listas y el gauge.
    let zona_fondo = Rect::new(body.x, body.y, body.width, frame.area().height - body.y);
    if let Some(logo) = logo {
        fondo_logo(frame, zona_fondo, logo, &pal);
    }

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
                            Style::default().fg(pal.texto),
                        ))
                        .border_style(Style::default().fg(pal.atenuado)),
                )
                .highlight_symbol("▶ ")
                .highlight_style(Style::default().bg(pal.texto).fg(pal.fondo));
            frame.render_stateful_widget(list, body, list_state);
        }
        Tab::Sugeridos => sugeridos(frame, body, list_state, sel, sug, pal),
    }

    // ── barra inferior (now playing) ─────────────────────────────
    let [gauge_area, footer] =
        Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(status);

    let now = player.titulo();
    let state = if player.current().is_none() && player.stream_name().is_none() {
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
        Span::styled(format!("{state} {now}"), Style::default().fg(pal.texto)),
        Span::styled(
            format!("  — vol {}%{rep}{shuf}", (player.volume() * 100.0).round()),
            Style::default().fg(pal.atenuado),
        ),
    ]);

    let dur = player.duration();
    let pos = player.position();
    // los streams online no tienen duración conocida: la barra muestra
    // el tiempo de conexión y el rótulo "en vivo" en vez de un avance.
    let (pct, etiqueta) = match dur {
        Some(d) if !d.is_zero() => (
            (pos.as_secs_f64() / d.as_secs_f64() * 100.0).clamp(0.0, 100.0) as u16,
            format!("{} / {}", format_hms(pos), format_hms(d)),
        ),
        _ => (0, format!("{} · en vivo", format_hms(pos))),
    };
    let gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_style(Style::default().fg(pal.atenuado))
                .title(info_line),
        )
        .gauge_style(Style::default().fg(pal.texto))
        .label(etiqueta)
        .percent(pct);
    frame.render_widget(gauge, gauge_area);

    // pie con los atajos principales.
    let ayuda = Line::from(Span::styled(
        "→/← pista · espacio pausa · f fuente · t tema · a aleatorio · r repetir · +/- volumen · s detener · q salir",
        Style::default().fg(pal.atenuado),
    ))
    .centered();
    frame.render_widget(Paragraph::new(ayuda), footer);
}

/// vista de sugerencias: radios (radio-browser) o temas (archive.org).
/// muestra la lista activa, la carga en curso y el error si lo hubo.
fn sugeridos(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    list_state: &mut ListState,
    sel: usize,
    sug: &SugEstado,
    pal: Paleta,
) {
    let SugEstado {
        fuente,
        consulta,
        radios,
        temas,
        rx,
        err,
    } = sug;
    let fuente_txt = fuente.texto();
    let items: Vec<ListItem> = match fuente {
        FuenteSug::Radios => radios
            .iter()
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(e.nombre.clone(), Style::default().fg(pal.texto)),
                    Span::styled(
                        format!("  · {} kbps {}", e.bitrate, e.codec.to_lowercase()),
                        Style::default().fg(pal.atenuado),
                    ),
                ]))
            })
            .collect(),
        FuenteSug::Temas => temas
            .iter()
            .map(|t| {
                ListItem::new(Line::from(Span::styled(
                    t.titulo.clone(),
                    Style::default().fg(pal.texto),
                )))
            })
            .collect(),
    };

    let total = items.len();
    let mut lineas_titulo = vec![format!(" Sugeridos — {fuente_txt}: «{consulta}» ")];
    if rx.is_some() {
        lineas_titulo.push(" buscando… ".to_owned());
    }
    if let Some(e) = err {
        lineas_titulo.push(format!(" ! {e} "));
    }

    let mut list = List::new(items)
        .block(
            Block::bordered()
                .title(Span::styled(
                    lineas_titulo.join("·"),
                    Style::default().fg(pal.texto),
                ))
                .title_bottom(Span::styled(
                    " f alterna la fuente · Enter conecta ",
                    Style::default().fg(pal.atenuado),
                ))
                .border_style(Style::default().fg(pal.atenuado)),
        )
        .highlight_symbol("▶ ")
        .highlight_style(Style::default().bg(pal.texto).fg(pal.fondo));

    if total == 0 && rx.is_none() && err.is_none() {
        // sin datos todavía: mensaje de bienvenida interno de la vista.
        list = List::new(vec![ListItem::new(Line::from(Span::styled(
            "sin sugerencias; pulsa f para recargar",
            Style::default().fg(pal.atenuado),
        )))]);
    } else {
        list_state.select(Some(sel.clamp(0, total.saturating_sub(1))));
    }
    frame.render_stateful_widget(list, area, list_state);
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// formato mm:ss (u h:mm:ss si supera una hora).
fn format_hms(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// eventos de alto nivel que emite la navegación de teclado.
/// ///el cli los detecta vía `parse_key_event`; la tui los mapea
/// directamente de crossterm. la gui (egui) seguirá este modelo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavEvent {
    /// ctrl+right (o ctrl+">"): vista siguiente.
    NextTab,
    /// ctrl+left (o ctrl+"<"): vista anterior.
    PrevTab,
    /// right: pista siguiente.
    NextTrack,
    /// left: pista anterior.
    PrevTrack,
}

/// traduce una línea cruda del terminal (modo cli) a un evento de
/// navegación. en modo canónico las secuencias llegan como líneas:
/// `ESC[1;5C` = ctrl+right, `ESC[1;5D` = ctrl+left,
/// `ESC[C` = right, `ESC[D` = left.
pub fn parse_key_event(line: &str) -> Option<NavEvent> {
    match line {
        "\u{1b}[1;5C" | "\u{1b}OC" => Some(NavEvent::NextTab),
        "\u{1b}[1;5D" | "\u{1b}OD" => Some(NavEvent::PrevTab),
        "\u{1b}[C" => Some(NavEvent::NextTrack),
        "\u{1b}[D" => Some(NavEvent::PrevTrack),
        _ => None,
    }
}
