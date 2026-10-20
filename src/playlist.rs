//! Cola de reproducción de nuva.
//!
//! Independiente del UI y del motor de audio: solo rutas y orden.
//! El orden canónico es el de `tracks`; `order` es la permutación
//! activa (identidad o barajada) y `pos` la posición DENTRO de esa
//! permutación.

use std::path::{Path, PathBuf};
use std::{fs, io::Write};

/// Modo de repetición.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    /// Al llegar al fin, se detiene.
    #[default]
    Off,
    /// Al llegar al fin, vuelve a empezar la cola.
    All,
    /// Repite la pista actual indefinidamente.
    One,
}

/// Cola de pistas con orden, barajado y repetición.
pub struct Queue {
    tracks: Vec<PathBuf>,
    /// Posiciones sobre `tracks` en el orden de reproducción activo.
    order: Vec<usize>,
    /// Índice dentro de `order` de la pista actual (None = nada cargado).
    pos: Option<usize>,
    repeat: Repeat,
}

impl Queue {
    pub fn new(tracks: Vec<PathBuf>) -> Self {
        let order = (0..tracks.len()).collect();
        Self { tracks, order, pos: None, repeat: Repeat::Off }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// true si el orden activo difiere del canónico.
    pub fn shuffled(&self) -> bool {
        self.order.iter().copied().ne(0..self.tracks.len())
    }

    /// Índice canónico de la pista actual.
    pub fn track_index(&self) -> Option<usize> {
        self.pos.map(|p| self.order[p])
    }

    /// Pista según el orden activo, si la posición es válida.
    pub fn current(&self) -> Option<&Path> {
        let i = self.pos.and_then(|p| self.order.get(p))?;
        Some(&self.tracks[*i])
    }

    /// Salta a una pista por índice canónico (0-based).
    pub fn select(&mut self, track_index: usize) {
        if track_index < self.tracks.len() {
            if let Some(p) = self.order.iter().position(|&o| o == track_index) {
                self.pos = Some(p);
            }
        }
    }

    /// Busca una pista: por número 1-based o por subcadena del nombre.
    pub fn find_by_name(&self, query: &str) -> Option<usize> {
        if let Ok(n) = query.parse::<usize>() {
            return (n >= 1 && n <= self.len()).then_some(n - 1);
        }
        let q = query.to_lowercase();
        self.tracks
            .iter()
            .position(|p| file_name(p).to_lowercase().contains(&q))
    }

    /// Pista siguiente según el orden y el modo de repetición.
    ///
    /// Devuelve `None` al terminar la cola con `Repeat::Off`.
    /// Con `Repeat::One` devuelve de nuevo la pista actual.
    pub fn next(&mut self) -> Option<&Path> {
        if self.is_empty() {
            return None;
        }
        if self.repeat == Repeat::One {
            return self.current();
        }
        let candidate = self.pos.map_or(0, |p| p + 1);
        if candidate < self.order.len() {
            self.pos = Some(candidate);
        } else if self.repeat == Repeat::All {
            self.pos = Some(0);
        } else {
            self.pos = None;
            return None;
        }
        self.current()
    }

    /// Pista anterior; en el inicio, `Repeat::All` la envía a la última,
    /// el resto se queda en la primera.
    pub fn prev(&mut self) -> Option<&Path> {
        if self.is_empty() {
            return None;
        }
        match self.pos {
            Some(p) if p > 0 => self.pos = Some(p - 1),
            Some(0) if self.repeat == Repeat::All => self.pos = Some(self.order.len() - 1),
            None => self.pos = Some(0),
            _ => {}
        }
        self.current()
    }

    /// Alterna barajado: al activarlo, la pista actual queda primera;
    /// al desactivarlo, se restaura el orden canónico conservando la
    /// pista actual.
    pub fn toggle_shuffle(&mut self) {
        let track_index = self.track_index().unwrap_or(0);
        if self.shuffled() {
            self.order = (0..self.tracks.len()).collect();
        } else {
            shuffle(&mut self.order);
        }
        self.select(track_index);
        if self.pos.is_none() && !self.is_empty() {
            self.pos = Some(0);
        }
    }

    /// Serializa la cola completa (orden canónico) en formato M3U.
    pub fn save_m3u(&self, path: &Path) -> Result<(), String> {
        let mut buf = String::from("#EXTM3U\n");
        for p in &self.tracks {
            if let Some(n) = p.file_name() {
                buf.push_str(&format!(
                    "#EXTINF:,{}\n{}\n",
                    n.to_string_lossy(),
                    p.display()
                ));
            }
        }
        fs::File::create(path)
            .and_then(|mut f| f.write_all(buf.as_bytes()))
            .map_err(|e| format!("no se pudo guardar {:?}: {e}", path.display()))
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Fisher–Yates con un LCG sembrado del reloj del sistema: el barajado
/// no necesita RNG criptográfica y así seguíamos sin dependencias
/// externas.
fn shuffle<T>(slice: &mut [T]) {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    for i in (1..slice.len()).rev() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = ((seed >> 33) % (i as u64 + 1)) as usize;
        slice.swap(i, j);
    }
}
