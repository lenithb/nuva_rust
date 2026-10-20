//! motor de reproducción de audio basado en `rodio`.
//! //! todo el audio se decodifica **en streaming** desde disco: nunca se
//! carga un archivo completo en ram (la memoria es plana aunque la
//! pista dure horas). en rodio 0.23 el sink de bajo nivel es
//! `DeviceSinkBuilder` (maneja el dispositivo) y `Player` (la cola).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

use crate::online;

/// reproductor de audio con controles básicos (play/pausa/stop/volumen/seek).
pub struct MusicPlayer {
    /// el dispositivo deja de sonar si esto se descarta; vive junto al
    /// reproductor.
    _device: MixerDeviceSink,
    player: Player,
    /// ruta cuando la fuente es local; none para streams.
    current: Option<PathBuf>,
    /// titulo visible cuando la fuente es online (radio o tema).
    stream_name: Option<String>,
    /// instante de conexión del stream (para su "posición").
    stream_since: Option<Instant>,
    current_duration: Option<Duration>,
    volume: f32,
}

impl MusicPlayer {
    /// crea el reproductor y abre el dispositivo de audio por defecto.
    pub fn new() -> Result<Self, String> {
        let mut device = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("no se pudo abrir el dispositivo de audio: {e}"))?;
        device.log_on_drop(false);
        let player = Player::connect_new(device.mixer());
        Ok(Self {
            _device: device,
            player,
            current: None,
            stream_name: None,
            stream_since: None,
            current_duration: None,
            volume: 1.0,
        })
    }

    /// carga y reproduce una pista, deteniendo la anterior.
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
        self.stream_name = None;
        self.stream_since = None;
        Ok(())
    }

    /// reproduce una fuente online: radio (stream de por vida) o un
    /// mp3 remoto. el nombre describe la pista que se muestra en la ui.
    pub fn play_stream(&mut self, url: &str, nombre: &str) -> Result<(), String> {
        let source = online::decodificador_de(url)?;
        self.player.clear();
        self.player.append(source);
        self.player.play(); // defensivo: tras vaciar la cola no debe quedar en pausa
        self.player.set_volume(self.volume);
        self.current = None;
        self.current_duration = None;
        self.stream_name = Some(nombre.to_owned());
        self.stream_since = Some(Instant::now());
        Ok(())
    }

    /// pausa la reproducción.
    pub fn pause(&self) {
        self.player.pause();
    }

    /// reanuda la reproducción.
    pub fn resume(&self) {
        self.player.play();
    }

    /// alterna pausa/reanudación.
    pub fn toggle(&mut self) {
        if self.player.is_paused() {
            self.resume();
        } else {
            self.pause();
        }
    }

    /// detiene la reproducción y descarga la pista actual.
    pub fn stop(&mut self) {
        self.player.stop();
        self.player.clear();
        self.current = None;
        self.stream_name = None;
        self.stream_since = None;
        self.current_duration = None;
    }

    /// salta a una posición absoluta de la pista.
    pub fn seek(&self, position: Duration) {
        let _ = self.player.try_seek(position);
    }

    /// ajusta el volumen (0.0–1.0) conservando el estado de reproducción.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.player.set_volume(self.volume);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// ruta de la pista cargada, si la hay (none para streams).
    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// titulo visible de la fuente online, si la hay.
    pub fn stream_name(&self) -> Option<&str> {
        self.stream_name.as_deref()
    }

    /// titulo a mostrar en la ui: stream online o pista local.
    pub fn titulo(&self) -> String {
        match (&self.stream_name, &self.current) {
            (Some(nombre), _) => nombre.clone(),
            (None, Some(path)) => crate::playlist::file_name(path),
            (None, None) => "nada en reproducción".into(),
        }
    }

    /// duración total de la pista cargada (según el decodificador).
    pub fn duration(&self) -> Option<Duration> {
        self.current_duration
    }

    /// posición actual de reproducción: en archivos, lo que reporta
    /// rodio; en streams, tiempo desde la conexión.
    pub fn position(&self) -> Duration {
        if self.current.is_some() {
            self.player.get_pos()
        } else {
            self.stream_since
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO)
        }
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    /// devuelve true si la pista cargada ya terminó de sonar.
    pub fn is_finished(&self) -> bool {
        self.player.empty()
    }
}
