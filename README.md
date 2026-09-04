# WindowDeck

WindowDeck busca convertir la pantalla de una Steam Deck en un monitor secundario real de Windows 11 mediante la red local.

El proyecto está en fase de prototipo: por ahora valida el protocolo y una conexión TCP manual con un patrón sintético pequeño. Todavía no crea un monitor virtual ni captura la pantalla. El HDMI del dock de Steam Deck es una salida, no una entrada.

## Requisitos

- Rust estable con Cargo.
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

El cliente muestra el patrón en la terminal y ambos procesos imprimen métricas. `Ctrl+C` detiene el host; el cliente termina al cerrarse la conexión.

## Comprobar

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Consulta [la hoja de ruta](WINDOWDECK_ROADMAP.md) para conocer el alcance y los hitos.

## Licencia

Disponible bajo licencia MIT o Apache 2.0, a elección del usuario.
