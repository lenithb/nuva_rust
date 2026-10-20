<div align="center">

  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-oscuro.png">
    <img src="assets/logo.png" alt="nuva" width="360">
  </picture>

Reproductor de música ligero para la terminal.

</div>

## Filosofía

- **Ligero por diseño**: streaming desde disco — un archivo nunca se carga
  completo en RAM (la memoria es plana aunque la pista dure horas). Lo mismo
  aplica a las radios online: el stream se descarga en bloques, nunca entero.
- **Dependencias mínimas**: motor de audio con solo los decoders que se usan
  (`mp3`, `flac`, `vorbis`, `wav`); nada de subsistemas que no se usan.
- **Terminal-first**: toda la funcionalidad accesible por teclado.

## Uso

```sh
cargo run                       # escanea el directorio actual
cargo run -- /ruta/a/musica     # escanea el directorio indicado
cargo run -- /ruta --cli        # modo consola clásico
```

### TUI (por defecto)

```
Ctrl+→ / Ctrl+←    cambia la vista (Sugeridos ↔ Cola)
→ / ←              pista siguiente / anterior
Enter              reproduce la pista seleccionada; en Sugeridos conecta el stream
j / k o ↑ / ↓      navega la lista
f                  en Sugeridos: alterna radios ↔ temas ↔ carpeta local
t                  alterna el tema: ultra oscuro (#000000) ↔ claro (#ffffff)
espacio            pausa / reanuda
a                  alterna barajado
r                  cicla repetición (off → todas → una)
+ / -              volumen ±5%
s                  detener
q / Esc            salir
```

La vista **Sugeridos** trae música online sin clave de API:

- `radio-browser.info` — radios del mundo, streaming directo mp3.
- `archive.org` — temas de acceso abierto (dominio público / CC), con la
  URL del mp3 resuelta al momento de conectar.

Nota: no hay decodificador AAC, así que las emisoras listadas como
`aac` no sonarán; elige alguna marcada `mp3`.

### Modo consola (`--cli`)

```
reproducir <n|nombre>    reproduce la pista n (desde 1) o la que contenga «nombre»
siguiente / anterior     cambia de pista (también ← / →)
pausar / continuar       pausa y reanuda
alternar                 cambia entre pausa y reproducción
detener                  detiene y descarga la pista actual
aleatorio                alterna el barajado de la cola
repetir [off|todas|una]  modo de repetición (sin argumento: cicla)
buscar <+s|-s|m:ss>      salto de tiempo (ej.: +15, -30, 2:10)
volumen [0-100]          ajusta (o consulta) el volumen
guardar <archivo>        guarda la cola como lista .m3u
radios <consulta>        busca radios online (ej.: radios lofi)
temas <consulta>         busca temas cc en archive.org (ej.: temas blues)
radio <n> / tema <n>     conecta la sugerencia nº n (streaming)
sugeridos / cola         cambia de vista (también Ctrl+← / Ctrl+→)
estado                   posición y duración de la pista actual
ayuda                    muestra esta ayuda
salir                    termina el programa
```

## Requisitos del sistema

En Linux, el audio usa ALSA vía `cpal`:

```sh
sudo apt-get install libasound2-dev   # Debian/Ubuntu
sudo dnf install alsa-lib-devel       # Fedora
```

## Compilación

```sh
cargo build --release   # binario ~2,3 MB con LTO y símbolos strip
```

## Estructura

- `src/main.rs` — arranque, escaneo de pistas y modo consola.
- `src/player.rs` — motor de audio (play/pausa/stop/volumen/seek/streams).
- `src/playlist.rs` — cola de reproducción (orden, barajado, repetición).
- `src/ui.rs` — modelo de vistas, navegación de teclado y TUI.
- `src/online.rs` — API de radios y archive.org + puente http→rodio.
- `assets/` — logo (svg maestro + png compacto incrustado en el binario,
  pintado como marca de agua del cuerpo y teñido por el tema).

## Hoja de ruta

- [x] Motor de audio
- [x] Fase A: limpieza (rodio 0.23, decoders opt-in, release optimizado)
- [x] Fase B: playlist (cola, shuffle, repeat, guardar M3U, auto-avance)
- [x] Fase C: interfaz TUI (ratatui 0.30, vistas Ctrl+flechas)
- [x] Fase D: música online gratis (radio-browser.info, Internet Archive)
- [x] Temas: ultra oscuro (`#000000` + blanco) y claro (`#ffffff` + negro)
- [x] Logo como marca de agua (png compacto, invertido según el tema)
