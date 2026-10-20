<div align="center">

# 🌒 nuva

Reproductor de música ligero para la terminal.

</div>

## Filosofía

- **Ligero por diseño**: streaming desde disco — un archivo nunca se carga
  completo en RAM (la memoria es plana aunque la pista dure horas).
- **Dependencias mínimas**: motor de audio con solo los decoders que se usan
  (`mp3`, `flac`, `vorbis`, `wav`); nada de subsistemas que no se usan.
- **Terminal-first**: toda la funcionalidad accesible por teclado.

## Uso

```sh
cargo run                       # escanea el directorio actual
cargo run -- /ruta/a/musica     # escanea el directorio indicado
```

Busca archivos `.mp3`, `.wav`, `.flac`, `.ogg` y `.oga` y muestra un menú:

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

En Linux, el audio usa ALSA vía `cpal`:

```sh
sudo apt-get install libasound2-dev   # Debian/Ubuntu
sudo dnf install alsa-lib-devel       # Fedora
```

## Compilación

```sh
cargo build --release   # binario ~1,9 MB con LTO y símbolos stripping
```

## Estructura

- `src/main.rs` — escaneo de pistas, menú y comandos de consola.
- `src/player.rs` — motor de audio (play/pausa/stop/volumen/seek).
- `src/playlist.rs` — *(en desarrollo)* cola, shuffle, repeat.
- `src/ui.rs` — *(en desarrollo)* interfaz TUI.

## Hoja de ruta

- [x] Motor de audio
- [x] Fase A: limpieza (rodio 0.23, decoders opt-in, release optimizado)
- [x] Fase B: playlist (cola, shuffle, repeat, guardar M3U, auto-avance)
- [ ] Fase C: interfaz TUI
- [ ] Fase D: música online gratis (radio-browser.info, Internet Archive)
- [ ] Assets, estilos y temas
