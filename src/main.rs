//! Reproductor de música por consola.
//!
//! Uso:
//!   cargo run                       # escanea el directorio actual
//!   cargo run -- /ruta/a/musica     # escanea el directorio indicado
//!
//! Comandos (ver `ayuda` dentro del programa).

mod player;

use std::collections::BTreeSet;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use player::MusicPlayer;

/// extensiones de audio soportadas por rodio (decoder default).
const AUDIO_EXTS: [&str; 6] = ["mp3", "wav", "flac", "ogg", "oga", "m4a"];

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let dir = env::args().nth(1).unwrap_or_else(|| ".".into());
    let tracks = scan_dir(Path::new(&dir))?;

    if tracks.is_empty() {
        return Err(format!(
            "no hay archivos de audio ({}) en {:?}",
            AUDIO_EXTS.join(", "),
            dir
        ));
    }

    println!("🎵 Reproductor Rust | {} pistas en {:?}", tracks.len(), dir);
    print_list(&tracks);
    println!("\nEscribe 'ayuda' para ver los comandos.\n");

    let mut player = MusicPlayer::new().map_err(|e| format!("audio no disponible: {e}"))?;
    let mut index: Option<usize> = None;
    let mut paused_by_user = false;

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            prompt();
            continue;
        }

        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or_default();
        let arg = parts.next().unwrap_or_default().trim();

        let quit = handle_command(
            cmd,
            arg,
            &mut player,
            &tracks,
            &mut index,
            &mut paused_by_user,
        );
        if quit {
            break;
        }
        prompt();
    }

    player.stop();
    println!("¡Hasta luego!");
    Ok(())
}

/// prompt y título persistentes entre comandos.
fn prompt() {
    print!("command> ");
    let _ = io::stdout().flush();
}

/// ejecuta un comando; devuelve true si pidió salir.
fn handle_command(
    cmd: &str,
    arg: &str,
    player: &mut MusicPlayer,
    tracks: &[PathBuf],
    index: &mut Option<usize>,
    paused_by_user: &mut bool,
) -> bool {
    match cmd {
        "reproducir" | "play" | "r" => {
            if arg.is_empty() {
                println!("Uso: reproducir <n|nombre>");
            } else {
                match parse_track(arg, tracks) {
                    Ok(i) => {
                        *index = Some(i);
                        *paused_by_user = false;
                        load(player, tracks, Some(i));
                    }
                    Err(e) => println!("{e}"),
                }
            }
        }
        "siguiente" | "n" => {
            let i = index.map_or(0, |i| (i + 1) % tracks.len());
            *index = Some(i);
            *paused_by_user = false;
            load(player, tracks, Some(i));
        }
        "anterior" | "b" => {
            let i = index.map_or(0, |i| (i + tracks.len() - 1) % tracks.len());
            *index = Some(i);
            *paused_by_user = false;
            load(player, tracks, Some(i));
        }
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
            *index = None;
        }
        "buscar" | "seek" => {
            if arg.is_empty() {
                println!("Uso: buscar <+s|-s|m:ss>  (ej.: +15, -30, 2:10)");
            } else {
                match parse_seek(arg, player.position()) {
                    Ok(t) => player.seek(t),
                    Err(e) => println!("{e}"),
                }
            }
        }
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
        "estado" | "e" => {
            let mark = if player.current().is_none() {
                "⏹"
            } else if player.is_finished() {
                "✓ (terminada)"
            } else if player.is_paused() {
                "⏸"
            } else {
                "▶"
            };
            println!(
                "{} {}  [{} / {}]",
                mark,
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
        "info" | "listar" | "l" => print_list(tracks),
        "ayuda" | "h" | "?" => print_help(),
        "salir" | "q" | "exit" => return true,
        _ => println!("Comando desconocido: {cmd:?} (usa 'ayuda')"),
    }
    false
}

/// carga y reproduce la pista en `index`, mostrando el título.
fn load(player: &mut MusicPlayer, tracks: &[PathBuf], index: Option<usize>) {
    let path = index.map(|i| tracks[i].clone());
    if let Some(path) = &path {
        match player.play(path) {
            Ok(()) => println!("♪ Reproduciendo: {}", file_name(path)),
            Err(e) => println!("{e}"),
        }
    } else {
        println!("Nada cargado.");
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

fn print_list(tracks: &[PathBuf]) {
    for (i, p) in tracks.iter().enumerate() {
        println!(" {:>3}. {}", i + 1, file_name(p));
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// resuelve el argumento de "reproducir": número (1-based) o subcadena del nombre.
fn parse_track(arg: &str, tracks: &[PathBuf]) -> Result<usize, String> {
    if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= tracks.len() {
            return Ok(n - 1);
        }
        return Err(format!("pista fuera de rango: hay {} temas", tracks.len()));
    }
    let q = arg.to_lowercase();
    tracks
        .iter()
        .position(|p| file_name(p).to_lowercase().contains(&q))
        .ok_or_else(|| format!("no se encontró ninguna pista con «{arg}»"))
}

/// interpreta el argumento de "buscar": "90", "2:10", "+15", "-30".
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

/// formato mm:ss (u h:mm:ss si supera una hora).
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
         \x20 pausar                  pausa la reproducción\n\
         \x20 continuar               reanuda después de una pausa\n\
         \x20 alternar                cambia entre pausa y reproducción\n\
         \x20 detener                 detiene y descarga la pista actual\n\
         \x20 siguiente               siguiente pista (al final vuelve a la primera)\n\
         \x20 anterior                pista anterior\n\
         \x20 buscar <+s|-s|m:ss>     salto de tiempo (ej.: +15, -30, 2:10)\n\
         \x20 volumen [0-100]         ajusta (o consulta) el volumen\n\
         \x20 estado                   posición y duración de la pista actual\n\
         \x20 info                     vuelve a listar las pistas\n\
         \x20 ayuda                   muestra esta ayuda\n\
         \x20 salir                   termina el programa"
    );
}
