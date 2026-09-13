# Desarrollo y diagnósticos

La ruta CPU usa frames del driver y libx264. Negocia unidades de acceso H.264
con el cliente integrado o MPEG-TS con los clientes anteriores; véase
[ADR 0019](adr/0019-cpu-integrated-player.md).
Las rutas históricas se conservan para reproducir sus mediciones; no se amplían
ni se eligen automáticamente. Véase [ADR 0016](adr/0016-supported-video-routes.md).

## Compilar y empaquetar Windows

Requisitos de desarrollo: Rust estable, herramientas C++ de Visual Studio y
WDK para compilar el driver completo. El uso del paquete no requiere Rust.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-media.ps1 -Download
powershell -NoProfile -ExecutionPolicy Bypass -File driver/windows-idd/build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -SkipBuild
```

El primer comando obtiene SDK con versión y SHA256 fijados, compila release y
copia FFmpeg/SDL junto a los binarios. `-Download` solo descarga archivos ausentes.
El segundo compila el auxiliar y el paquete del driver; no instala la DLL.
El ZIP queda en `target/package-<identificador>/WindowDeck-0.2.0-windows-x64.zip` e incluye versiones,
hashes, licencias y fuentes. La validación en un Windows limpio sigue pendiente.
`scripts/package-windows.ps1 -Release` exige un árbol limpio e incluye las
fuentes del commit exacto. El empaquetado normal solo necesita compilar el
auxiliar con `driver/windows-idd/build.ps1 -ControlOnly`; no necesita reconstruir
ni instalar el driver. `scripts/test-windows-package.ps1 -Archive RUTA.zip`
verifica la extracción, hashes, ejecutables, multimedia y acceso directo con
un PATH limitado al paquete y Windows.

Las etiquetas `v<versión>` activan `.github/workflows/release.yml`: CI en ambas
plataformas, Flatpak, paquete Windows y publicación conjunta con SHA256.
La [guía de distribución](steamdeck-distribution.md) documenta las descargas,
el instalador y los límites de la integración con Steam.

Para ejecutar desde el repositorio, abre `target/release/windowdeck-launcher.exe`
después de compilar. `cargo build --locked --release -p windowdeck-launcher`
compila únicamente el panel, sin necesitar FFmpeg/SDL ni WDK. Para iniciar una
sesión sí hacen falta el host, FFmpeg, sus DLL y el auxiliar del driver.
El panel utiliza CPU incluso si el binario admite `native-media`.

## Ruta CPU manual

Con el driver instalado, inicia el broker en una terminal **como administrador**:

```powershell
.\target\windows-idd\windowdeck-display.exe --frame-broker
```

En una terminal normal, añade los binarios incluidos al PATH de esa terminal e
inicia el host recomendado:

```powershell
$env:Path = (Join-Path $PWD 'target/release') + ';' + $env:Path
.\target\release\windowdeck-host.exe --driver-h264 0.0.0.0:48150
```

El panel hace esta preparación automáticamente. Sin argumentos, el host utiliza
también CPU en `0.0.0.0:48150`; una dirección suelta ya no inicia el patrón RGB332.
Si el auxiliar está fuera de las ubicaciones habituales, configura
`WINDOWDECK_DISPLAY_EXE`. El broker pertenece a la misma sesión de Windows.

El cliente habitual se abre con `flatpak run io.github.ik3rurru.WindowDeck`.
Con el cliente nuevo, la IP manual usa `IP_DEL_PC:48150 --native`; sin dirección,
el cliente con `native-media` selecciona automáticamente el reproductor integrado.
`--h264-test` con IP manual y `--ffplay` permiten comprobar MPEG-TS. Los clientes
anteriores conservan `IP_DEL_PC:48150 --h264-test`.

## CLI de diagnóstico

```powershell
.\target\release\windowdeck-host.exe diag --help
```

| Comando tras `windowdeck-host` | Función y requisitos |
| --- | --- |
| `diag capture [N]` | Recibe una textura D3D11 del monitor N y termina. N comienza en 1. |
| `diag encode [N]` | Codifica 60 frames en memoria mediante la prueba Media Foundation existente. |
| `diag driver-frames` | Valida muestras de 120 frames CPU del driver; requiere `--frame-broker`. |
| `diag gpu-frames` | Valida 120 texturas compartidas; requiere `--gpu-frame-broker`. |
| `diag gpu-encode [ENCODER]` | Ensayo integrado sintético en Windows con `native-media`; acepta `auto`, `libx264`, `h264_amf`, `h264_nvenc`. |

Los ensayos de frames activan un monitor y muestran su patrón conocido; ejecutarlos
sin otra sesión activa. El ensayo sintético del encoder no captura el escritorio.
Los contadores y muestras no equivalen a una medición de latencia visual.

Para repetir los arneses del driver, compila también el host de depuración con
`cargo build -p windowdeck-host`. `driver/windows-idd/test-driver-frames.ps1`
utiliza `diag driver-frames`; con `-Gpu`, `diag gpu-frames`.
`test-auto-host.ps1 -DriverCpu` mantiene la ruta CPU. Sin ese switch prueba WGC
mediante `diag auto-virtual-h264`. Conservan sus requisitos de brokers y de una
sesión inactiva; cambiar la CLI no cambia el procedimiento de aceptación.

## Referencias históricas congeladas

| Comando tras `windowdeck-host` | Referencia |
| --- | --- |
| `diag pattern [DIRECCION]` | Patrón RGB332, 128 × 80, sin driver; funciona en Windows/Linux. |
| `diag capture-stream N [DIRECCION]` | Vista previa RGB332 mediante Windows Graphics Capture. |
| `diag h264-stream N [DIRECCION]` | Captura física `ddagrab` y FFmpeg H.264. |
| `diag virtual-h264 [DIRECCION]` | WGC sobre WindowDeck activado previamente con `windowdeck-display --run`. |
| `diag auto-virtual-h264 [DIRECCION]` | WGC y monitor ligado a la sesión; requiere `windowdeck-display --broker`. |

La dirección predeterminada es `0.0.0.0:48150`. RGB332 utiliza el cliente con
`IP_DEL_PC:48150`; las rutas H.264 requieren además `--h264-test`.
Para WGC se conserva FFmpeg con `gfxcapture`, comprobado en la versión incluida.
Las instrucciones y resultados originales permanecen fechados en
[testing.md](testing.md) y los ADR 0003–0014.

Migración de la CLI anterior: `--capture-test` → `diag capture`, `--encode-test`
→ `diag encode`, `--driver-frame-test` → `diag driver-frames`, `--gpu-frame-test`
→ `diag gpu-frames`, `--gpu-self-test` → `diag gpu-encode`, `--capture`
→ `diag capture-stream`, `--h264` → `diag h264-stream`, `--virtual-h264`
→ `diag virtual-h264`, `--auto-virtual-h264` → `diag auto-virtual-h264`.
El patrón que antes se iniciaba sin flags pasa a `diag pattern`.
Los flags antiguos producen un error con la ayuda de migración; no se mantienen
dos despachadores de las mismas pruebas.

## Ruta GPU experimental

```powershell
.\target\release\windowdeck-launcher.exe --native
```

Esta selección exige `native-media`; el panel normal no la activa por detectar
la función compilada. El host usa `--driver-native-h264`, con brokers GPU y CPU
para poder negociar el respaldo de clientes antiguos. El cliente integrado se
selecciona con `windowdeck-client IP_DEL_PC:48150 --native`.

`WINDOWDECK_H264_ENCODER` permite experimentar sin cambiar el perfil normal:
la ruta integrada admite `auto`, `libx264`, `h264_amf` y `h264_nvenc`; la ruta
de FFmpeg externo admite `libx264`, `h264_amf`, `h264_nvenc` y `h264_mf`.
No se presupone que la codificación hardware mejore el recorrido completo.

## Comprobaciones

```powershell
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
. ./scripts/build-media.ps1 -PrepareOnly
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
target/release/windowdeck-client.exe --media-self-test
python scripts/test-native-client.py --client target/release/windowdeck-client.exe
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-launcher.ps1
```

La CLI se valida sin activar el monitor. La aceptación del driver y de la ruta
GPU requiere hardware real. Los requisitos de una hora de vídeo, latencia visual
y cambio de usuario siguen abiertos; véase el [roadmap](../WINDOWDECK_ROADMAP.md).

La prueba del panel guarda captura, controles accesibles, tiempo de arranque,
memoria, DPI y cierre en `target/launcher-smoke-<identificador>/`. No inicia
host ni broker ni modifica firewall. Se ejecuta también en CI de Windows.
`WINDOWDECK_LOG_DIR` permite cambiar la carpeta de registros de las sesiones
durante pruebas; no modifica variables globales. El diseño del lanzador está
en [ADR 0018](adr/0018-rust-launcher.md).

Con el driver instalado y sin otra instancia abierta, `scripts/test-launcher.ps1
-Session` comprueba Iniciar/UAC/espera/Detener con el host real; prepara las dos
reglas privadas del firewall. `-Session -TerminatePanel` termina su propio panel
después del arranque y verifica la recogida de host y broker. Son ensayos locales
sin cliente: no sustituyen la aceptación visual del vídeo en la Deck.
