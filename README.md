# reproductor-rust

Reproductor de música por consola escrito en Rust con [`rodio`](https://crates.io/crates/rodio).

## Uso

```sh
cargo run                       # escanea el directorio actual
cargo run -- /ruta/a/musica     # escanea el directorio indicado
```

Busca archivos `.mp3`, `.wav`, `.flac`, `.ogg`, `.oga` y `.m4a` y muestra un menú:

```
Comandos:
 reproducir <n|nombre>   reproduce la pista n (desde 1) o la que contenga «nombre»
 pausar                  pausa la reproducción
 continuar               reanuda después de una pausa
 alternar                cambia entre pausa y reproducción
 detener                 detiene y descarga la pista actual
 siguiente               siguiente pista (al final vuelve a la primera)
 anterior                pista anterior
 buscar <+s|-s|m:ss>     salto de tiempo (ej.: +15, -30, 2:10)
 volumen [0-100]         ajusta (o consulta) el volumen
 estado                  posición y duración de la pista actual
 info                    vuelve a listar las pistas
 ayuda                   muestra esta ayuda
 salir                   termina el programa
```

## Requisitos del sistema

En Linux, `rodio`/`cpal` requiere la librería ALSA:

- **Con sudo (recomendado):**
  ```sh
  sudo apt-get install libasound2-dev
  ```
  (`pkg-config` la detectará automáticamente.)

- **Sin sudo (workaround actual):** la configuración en
  `.cargo/config.toml` apunta `PKG_CONFIG_PATH` a una copia de la
  librería extraída del `.deb` en disco temporal. Nota: esa copia no
  sobrevive a un reinicio; si desaparece, instala `libasound2-dev`
  o repite la extracción (el README del proyecto incluye el detalle).

## Estructura

- `src/main.rs` — escaneo de pistas, menú y comandos de consola.
- `src/player.rs` — motor de audio (play/pausa/stop/volumen/seek).
- `src/playlist.rs`, `src/ui.rs` — módulos reservados para funcionalidad futura.
