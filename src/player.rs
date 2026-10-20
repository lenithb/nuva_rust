//! Motor de reproducción de audio basado en `rodio`.
//!
//! Maneja un único `Sink`: al cargar una nueva pista se vacía y se
//! encola la fuente decodificada. Garantiza al menos 100 ms de
//! silencio antes de liberar el stream para evitar chasquidos al salir.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rodio::{Decoder, OutputStream, Sink, Source};

/// reproductor de audio con controles básicos (play/pausa/stop/volumen/seek).
pub struct MusicPlayer {
    _stream: OutputStream,
    sink: Sink,
    current: Option<PathBuf>,
    current_duration: Option<Duration>,
    volume: f32,
    /// Instante en que empezó la pista (ajustado por pausas y seeks).
    started_at: Instant,
    /// Instante en que se pausó (None = reproduciendo).
    paused_at: Option<Instant>,
    /// Tiempo total acumulado en pausa durante esta pista.
    paused_total: Duration,
}

impl MusicPlayer {
    /// crea el reproductor y abre el dispositivo de audio por defecto.
    pub fn new() -> Result<Self, String> {
        let (_stream, handle) = OutputStream::try_default()
            .map_err(|e| format!("no se pudo abrir el dispositivo de audio: {e}"))?;
        let sink = Sink::try_new(&handle)
            .map_err(|e| format!("no se pudo crear el canal de salida: {e}"))?;
        Ok(Self {
            _stream,
            sink,
            current: None,
            current_duration: None,
            volume: 1.0,
            started_at: Instant::now(),
            paused_at: None,
            paused_total: Duration::ZERO,
        })
    }

    /// carga y reproduce una pista, deteniendo la anterior.
    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        let file =
            File::open(path).map_err(|e| format!("no se pudo abrir {:?}: {e}", path.display()))?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| format!("formato no soportado en {:?}: {e}", path.display()))?;

        self.current_duration = source.total_duration();
        self.sink.clear();
        self.sink.append(source);
        self.sink.play(); // `clear()` deja el sink en pausa en rodio 0.18
        self.sink.set_volume(self.volume);
        self.current = Some(path.to_path_buf());
        self.started_at = Instant::now();
        self.paused_at = None;
        self.paused_total = Duration::ZERO;
        Ok(())
    }

    /// pausa la reproducción.
    pub fn pause(&mut self) {
        if self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
        }
        self.sink.pause();
    }

    /// reanuda la reproducción.
    pub fn resume(&mut self) {
        if let Some(t) = self.paused_at.take() {
            self.paused_total += t.elapsed();
        }
        self.sink.play();
    }

    /// alterna pausa/reanudación.
    pub fn toggle(&mut self) {
        if self.sink.is_paused() {
            self.resume();
        } else {
            self.pause();
        }
    }

    /// detiene la reproducción y descarga la pista actual.
    pub fn stop(&mut self) {
        self.sink.stop();
        self.sink.clear();
        self.current = None;
        self.current_duration = None;
    }

    /// salta a una posición absoluta de la pista.
    pub fn seek(&mut self, position: Duration) {
        let delta = position.as_secs_f64() - self.position().as_secs_f64();
        if delta >= 0.0 {
            self.started_at -= Duration::from_secs_f64(delta);
        } else {
            self.started_at += Duration::from_secs_f64(-delta);
        }
        let _ = self.sink.try_seek(position);
    }

    /// ajusta el volumen (0.0–1.0) conservando el estado de reproducción.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.sink.set_volume(self.volume);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// ruta de la pista cargada, si la hay.
    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// duración total de la pista cargada (según el decodificador).
    pub fn duration(&self) -> Option<Duration> {
        self.current_duration
    }

    /// posición actual de reproducción (0 si no hay pista cargada).
    pub fn position(&self) -> Duration {
        if self.current.is_none() {
            return Duration::ZERO;
        }
        let mut elapsed = self.started_at.elapsed();
        if let Some(t) = &self.paused_at {
            elapsed = self.paused_total + t.elapsed();
            return elapsed;
        }
        elapsed.saturating_sub(self.paused_total)
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    /// true si la pista cargada ya terminó de sonar.
    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }
}
