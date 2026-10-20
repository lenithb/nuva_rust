//! Motor de reproducción de audio basado en `rodio`.
//!
//! Todo el audio se decodifica **en streaming** desde disco: nunca se
//! carga un archivo completo en RAM (la memoria es plana aunque la
//! pista dure horas). En rodio 0.23 el sink de bajo nivel es
//! `DeviceSinkBuilder` (maneja el dispositivo) y `Player` (la cola).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

/// Reproductor de audio con controles básicos (play/pausa/stop/volumen/seek).
pub struct MusicPlayer {
    /// El dispositivo deja de sonar si esto se descarta, así que vive
    /// junto al reproductor.
    _device: MixerDeviceSink,
    player: Player,
    current: Option<PathBuf>,
    current_duration: Option<Duration>,
    volume: f32,
}

impl MusicPlayer {
    /// Crea el reproductor y abre el dispositivo de audio por defecto.
    pub fn new() -> Result<Self, String> {
        let mut device = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("no se pudo abrir el dispositivo de audio: {e}"))?;
        device.log_on_drop(false);
        let player = Player::connect_new(device.mixer());
        Ok(Self {
            _device: device,
            player,
            current: None,
            current_duration: None,
            volume: 1.0,
        })
    }

    /// Carga y reproduce una pista, deteniendo la anterior.
    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        let file =
            File::open(path).map_err(|e| format!("no se pudo abrir {:?}: {e}", path.display()))?;
        let source = Decoder::try_from(file)
            .map_err(|e| format!("formato no soportado en {:?}: {e}", path.display()))?;

        self.current_duration = source.total_duration();
        self.player.clear();
        self.player.append(source);
        self.player.play(); // defensivo: tras vaciar la cola no debe quedar en pausa
        self.player.set_volume(self.volume);
        self.current = Some(path.to_path_buf());
        Ok(())
    }

    /// Pausa la reproducción.
    pub fn pause(&self) {
        self.player.pause();
    }

    /// Reanuda la reproducción.
    pub fn resume(&self) {
        self.player.play();
    }

    /// Alterna pausa/reanudación.
    pub fn toggle(&mut self) {
        if self.player.is_paused() {
            self.resume();
        } else {
            self.pause();
        }
    }

    /// Detiene la reproducción y descarga la pista actual.
    pub fn stop(&mut self) {
        self.player.stop();
        self.player.clear();
        self.current = None;
        self.current_duration = None;
    }

    /// Salta a una posición absoluta de la pista.
    pub fn seek(&self, position: Duration) {
        let _ = self.player.try_seek(position);
    }

    /// Ajusta el volumen (0.0–1.0) conservando el estado de reproducción.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.player.set_volume(self.volume);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Ruta de la pista cargada, si la hay.
    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// Duración total de la pista cargada (según el decodificador).
    pub fn duration(&self) -> Option<Duration> {
        self.current_duration
    }

    /// Posición actual de reproducción (0 si no hay pista cargada).
    pub fn position(&self) -> Duration {
        if self.current.is_none() {
            return Duration::ZERO;
        }
        self.player.get_pos()
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    /// Devuelve true si la pista cargada ya terminó de sonar.
    pub fn is_finished(&self) -> bool {
        self.player.empty()
    }
}
