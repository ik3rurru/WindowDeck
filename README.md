# WindowDeck

WindowDeck busca convertir la pantalla de una Steam Deck en un monitor secundario real de Windows 11 mediante la red local.

El proyecto está en fase de prototipo: valida el protocolo, una conexión TCP manual y la captura de una pantalla real de Windows. También puede codificar, enviar y reproducir una prueba H.264, pero todavía no ofrece vídeo comprimido continuo ni crea un monitor virtual. El HDMI del dock de Steam Deck es una salida, no una entrada.

## Requisitos

- Rust estable con Cargo.
- FFplay disponible en `PATH` para la prueba H.264 visible.
- Dos equipos en la misma red local o dos terminales en el mismo equipo.
- El puerto TCP elegido permitido por el firewall de Windows; el predeterminado es `48150`.

## Ejecutar

En Windows:

```powershell
cargo run -p windowdeck-host -- 0.0.0.0:48150
```

En SteamOS, sustituye `IP_DEL_PC` por la dirección local de Windows:

```bash
cargo run -p windowdeck-client -- IP_DEL_PC:48150
```

Añade `--fullscreen` para abrir directamente a pantalla completa:

```bash
cargo run -p windowdeck-client -- IP_DEL_PC:48150 --fullscreen
```

El cliente abre una ventana con el patrón; el número superior indica el frame y el inferior los milisegundos transcurridos en la sesión. Si el host se reinicia, la ventana permanece abierta y reconecta automáticamente. Ambos procesos imprimen métricas en la terminal. F11 alterna la pantalla completa, Escape vuelve al modo ventana y cerrar la ventana detiene el cliente. Pulsa `Ctrl+C` para detener el host.

Para comprobar la captura real de la pantalla principal en Windows sin enviarla todavía por red:

```powershell
cargo run -p windowdeck-host -- --capture-test
```

Puedes añadir el índice de otro monitor, por ejemplo `--capture-test 2`. La prueba confirma la recepción de una textura D3D11 y termina tras el primer frame.

Para transmitir una vista previa del primer monitor, inicia el host así y usa el cliente normalmente:

```powershell
cargo run -p windowdeck-host -- --capture 1 0.0.0.0:48150
```

Esta ruta temporal reduce la captura a 128 × 80 y RGB332 antes de enviarla. Sirve para validar el recorrido completo; H.264 y la resolución final pertenecen al siguiente hito.

Para comprobar el encoder H.264 de Windows sin guardar ni enviar el contenido de pantalla:

```powershell
cargo run -p windowdeck-host -- --encode-test
```

Puedes indicar otro monitor, por ejemplo `--encode-test 2`. La prueba codifica 60 frames a 30 FPS y 4 Mbps en memoria, muestra el tamaño resultante y termina. Usa la resolución actual de la pantalla.

Para probar el mismo segmento H.264 a través de la red, inicia el host:

```powershell
cargo run -p windowdeck-host -- --h264 1 0.0.0.0:48150
```

Y ejecuta el receptor desde el otro equipo o una segunda terminal:

```bash
cargo run -p windowdeck-client -- IP_DEL_PC:48150 --h264-test
```

El cliente reensambla los fragmentos, comprueba el MP4 con H.264, lo ofrece a FFplay desde memoria mediante un servidor local y termina al cerrar el vídeo. Añade `--fullscreen` si quieres verlo a pantalla completa. No se guarda la pantalla en disco. Es una prueba por lotes; la emisión continua de baja latencia es el siguiente paso.

## Comprobar

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Consulta [la hoja de ruta](WINDOWDECK_ROADMAP.md) para conocer el alcance y los hitos.

## Licencia

Disponible bajo licencia MIT o Apache 2.0, a elección del usuario.
