//! nuva — reproductor de música ligero para la consola.
//!
//! Uso:
//!   nuva                       escanea el directorio actual
//!   nuva /ruta/a/musica        escanea el directorio indicado
//!   nuva --cli                 modo consola clásico
//!
//! Navegación: Ctrl+izquierda/derecha cambia entre las vistas
//! (Sugeridos y Cola); izquierda/derecha pasa a la pista
//! anterior/siguiente. En modo consola lo mismo funciona,
//! además de los comandos `anterior`/`siguiente`.

mod player;
mod playlist;
mod ui;

use std::collections::BTreeSet;
use std::env;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use player::MusicPlayer;
use playlist::{Queue, Repeat};
use ui::{NavEvent, Tab};

/// Extensiones de audio soportadas por rodio (decoders opt-in).
const AUDIO_EXTS: [&str; 5] = ["mp3", "wav", "flac", "ogg", "oga"];
/// Intervalo de sondeo del bucle de eventos (auto-avance).
const POLL: Duration = Duration::from_millis(200);

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let cli_mode = args.iter().any(|a| a == "--cli");
    let dir = args.first().map(String::as_str).unwrap_or(".");
    let tracks = scan_dir(Path::new(dir))?;

    if tracks.is_empty() {
        return Err(format!(
            "no hay archivos de audio ({}) en {:?}",
            AUDIO_EXTS.join(", "),
            dir
        ));
    }

    if cli_mode {
        run_cli(tracks)
    } else {
        ui::run_tui(Queue::new(tracks))
    }
}

/// Modo consola clásico (nuva --cli).
fn run_cli(tracks: Vec<PathBuf>) -> Result<(), String> {
    println!("nuva — {} pistas (modo consola; la TUI arranca sin --cli)\n", tracks.len());
    let mut queue = Queue::new(tracks);
    let mut tab = Tab::Playlists;
    print_tab(tab, &queue);

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if tx.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });

    let mut player = MusicPlayer::new().map_err(|e| format!("audio no disponible: {e}"))?;
    let mut paused_by_user = false;
    let mut quit = false;

    // Bucle de eventos: la entrada se lee en un hilo aparte y este
    // bucle sondea cada POLL ms para avanzar pistas al terminar.
    loop {
        // Auto-avance cuando la pista terminó (salvo pausa del usuario).
        if !paused_by_user && player.current().is_some() && player.is_finished() {
            match queue.next() {
                Some(path) => play_now(&mut player, path),
                None => {
                    player.stop();
                    println!("— fin de la cola —");
                }
            }
        }

        match rx.recv_timeout(POLL) {
            Ok(line) => {
                if let Some(event) = ui::parse_key_event(&line) {
                    dispatch_nav(event, &mut tab, &mut queue, &mut player, &mut paused_by_user);
                } else {
                    quit |= handle_command(
                        &line,
                        &mut player,
                        &mut tab,
                        &mut queue,
                        &mut paused_by_user,
                    );
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if quit {
            break;
        }
    }

    player.stop();
    println!("Hasta luego.");
    Ok(())
}

fn dispatch_nav(
    event: NavEvent,
    tab: &mut Tab,
    queue: &mut Queue,
    player: &mut MusicPlayer,
    paused: &mut bool,
) {
    match event {
        NavEvent::NextTab => {
            *tab = tab.next();
            print_tab(*tab, queue);
        }
        NavEvent::PrevTab => {
            *tab = tab.prev();
            print_tab(*tab, queue);
        }
        NavEvent::NextTrack => track_step(player, queue, paused, true),
        NavEvent::PrevTrack => track_step(player, queue, paused, false),
    }
}

/// Encabezado de la vista activa.
fn print_tab(tab: Tab, queue: &Queue) {
    match tab {
        Tab::Sugeridos => println!("Sugeridos (próximamente: radio-browser.info / archive.org)"),
        Tab::Playlists => println!(
            "Cola ({} pistas, repetición: {:?}, barajado: {})",
            queue.len(),
            queue.repeat(),
            if queue.shuffled() { "sí" } else { "no" }
        ),
    }
}

/// Reproduce por ruta e informa el título.
fn play_now(player: &mut MusicPlayer, path: &Path) {
    match player.play(path) {
        Ok(()) => println!("♪ Reproduciendo: {}", file_name(path)),
        Err(e) => println!("{e}"),
    }
}

/// Siguiente/anterior pista (flechas o comandos).
fn track_step(player: &mut MusicPlayer, queue: &mut Queue, paused: &mut bool, forward: bool) {
    if queue.is_empty() {
        println!("La cola está vacía.");
        return;
    }
    let track = if forward { queue.next() } else { queue.prev() };
    match track {
        Some(path) => {
            *paused = false;
            play_now(player, path);
        }
        None => println!("— fin de la cola (usa 'repetir todas' para cabear) —"),
    }
}

fn scan_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = BTreeSet::new();
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("no se pudo leer {:?}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let matches = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if matches {
                found.insert(path);
            }
        }
    }
    Ok(found.into_iter().collect())
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Ejecuta un comando por línea; devuelve true si pidió salir.
fn handle_command(
    line: &str,
    player: &mut MusicPlayer,
    tab: &mut Tab,
    queue: &mut Queue,
    paused_by_user: &mut bool,
) -> bool {
    let mut parts = line.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or_default();
    let arg = parts.next().unwrap_or_default().trim();
    if cmd.is_empty() {
        return false;
    }

    match cmd {
        "reproducir" | "play" | "r" if !arg.is_empty() => match queue.find_by_name(arg) {
            Some(i) => {
                queue.select(i);
                *paused_by_user = false;
                play_now(player, queue.current().unwrap());
            }
            None => println!("no se encontró ninguna pista con «{arg}»"),
        },
        "reproducir" | "play" | "r" => println!("Uso: reproducir <n|nombre>"),
        "siguiente" | "n" => track_step(player, queue, paused_by_user, true),
        "anterior" | "b" => track_step(player, queue, paused_by_user, false),
        "pausar" | "p" => {
            player.pause();
            *paused_by_user = true;
        }
        "continuar" | "c" => {
            player.resume();
            *paused_by_user = false;
        }
        "alternar" | "pausa" => {
            player.toggle();
            *paused_by_user = player.is_paused();
        }
        "detener" | "s" => {
            player.stop();
        }
        "aleatorio" | "a" => {
            queue.toggle_shuffle();
            println!("Barajado: {}", if queue.shuffled() { "sí" } else { "no" });
        }
        "repetir" | "rep" => match arg {
            "off" | "0" => queue.set_repeat(Repeat::Off),
            "todas" | "all" | "1" => queue.set_repeat(Repeat::All),
            "una" | "one" | "2" => queue.set_repeat(Repeat::One),
            _ => {
                if arg.is_empty() {
                    // cicla Off → All → One
                    let next = match queue.repeat() {
                        Repeat::Off => Repeat::All,
                        Repeat::All => Repeat::One,
                        Repeat::One => Repeat::Off,
                    };
                    queue.set_repeat(next);
                } else {
                    println!("Uso: repetir [off|todas|una]");
                    return false;
                }
            }
        },
        "guardar" | "g" if !arg.is_empty() => match queue.save_m3u(Path::new(arg)) {
            Ok(()) => println!("Cola guardada en {arg}"),
            Err(e) => println!("{e}"),
        },
        "sugeridos" => {
            *tab = Tab::Sugeridos;
            print_tab(*tab, queue);
        }
        "playlist" | "cola" => {
            *tab = Tab::Playlists;
            print_tab(*tab, queue);
        }
        "estado" | "e" => {
            let estado = if player.current().is_none() {
                "sin pista".to_owned()
            } else if player.is_finished() {
                "terminada".to_owned()
            } else if player.is_paused() {
                "pausa".to_owned()
            } else {
                "reproduciendo".to_owned()
            };
            println!(
                "{} {}  [{} / {}]",
                estado,
                player
                    .current()
                    .map(file_name)
                    .unwrap_or_else(|| "nada en reproducción".into()),
                format_hms(player.position()),
                player
                    .duration()
                    .map(format_hms)
                    .unwrap_or_else(|| "--:--".into())
            );
        }
        "buscar" | "seek" => match (arg, player.position()) {
            ("", _) => println!("Uso: buscar <+s|-s|m:ss>  (ej.: +15, -30, 2:10)"),
            (a, pos) => match parse_seek(a, pos) {
                Ok(t) => player.seek(t),
                Err(e) => println!("{e}"),
            },
        },
        "volumen" | "vol" | "v" => {
            if arg.is_empty() {
                println!("Volumen actual: {}%", (player.volume() * 100.0).round());
            } else {
                match arg.parse::<f32>() {
                    Ok(vol) if (0.0..=100.0).contains(&vol) => {
                        player.set_volume(vol / 100.0);
                        println!("Volumen: {}%", (player.volume() * 100.0).round());
                    }
                    _ => println!("Volumen inválido; usa un número entre 0 y 100."),
                }
            }
        }
        "info" | "listar" | "l" => {
            *tab = Tab::Playlists;
            print_tab(*tab, queue);
        }
        "ayuda" | "h" | "?" => print_help(),
        "salir" | "q" | "exit" => return true,
        _ => println!("Comando desconocido: {cmd:?} (usa 'ayuda')"),
    }
    false
}

/// Interpreta el argumento de "buscar": "90", "2:10", "+15", "-30".
fn parse_seek(arg: &str, position: Duration) -> Result<Duration, String> {
    let secs: f64 = if let Some(rest) = arg.strip_prefix('+') {
        position.as_secs_f64() + rest.parse::<f64>().map_err(bad_seek)?
    } else if let Some(rest) = arg.strip_prefix('-') {
        (position.as_secs_f64() - rest.parse::<f64>().map_err(bad_seek)?).max(0.0)
    } else {
        match arg.split_once(':') {
            Some((m, s)) => {
                m.parse::<f64>().map_err(bad_seek)? * 60.0 + s.parse::<f64>().map_err(bad_seek)?
            }
            None => arg.parse().map_err(bad_seek)?,
        }
    };
    if secs < 0.0 {
        return Err("la posición no puede ser negativa".into());
    }
    Ok(Duration::from_secs_f64(secs))
}

fn bad_seek(_: std::num::ParseFloatError) -> String {
    "formato de tiempo inválido (ejemplos: 90, 2:10, +15, -30)".into()
}

/// Formato mm:ss (u h:mm:ss si supera una hora).
fn format_hms(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn print_help() {
    println!(
        "Comandos:\n\
         \x20 reproducir <n|nombre>   reproduce la pista n (desde 1) o la que contenga «nombre»\n\
         \x20 siguiente / anterior    cambia de pista (también ← / →)\n\
         \x20 pausar / continuar      pausa y reanuda\n\
         \x20 alternar                cambia entre pausa y reproducción\n\
         \x20 detener                 detiene y descarga la pista actual\n\
         \x20 aleatorio               alterna el barajado de la cola\n\
         \x20 repetir [off|todas|una] modo de repetición (sin argumento: cicla)\n\
         \x20 buscar <+s|-s|m:ss>     salto de tiempo (ej.: +15, -30, 2:10)\n\
         \x20 volumen [0-100]         ajusta (o consulta) el volumen\n\
         \x20 guardar <archivo>       guarda la cola como lista .m3u\n\
         \x20 sugeridos / cola        cambia de vista (también Ctrl+← / Ctrl+→)\n\
         \x20 estado                  posición y duración de la pista actual\n\
         \x20 ayuda                   muestra esta ayuda\n\
         \x20 salir                   termina el programa"
    );
}
