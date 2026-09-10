# WindowDeck — Roadmap de consolidación técnica

## Objetivo

Este documento complementa `WINDOWDECK_ROADMAP.md`. No añade funcionalidad nueva:
su objetivo es reducir la superficie de mantenimiento actual (rutas
experimentales duplicadas, lenguajes fuera del workspace, decisiones de
arquitectura pendientes) antes de seguir avanzando hacia los Hitos 5–7 del
roadmap original. Se ejecutará por entregas pequeñas, coordinando los cambios
de CLI, lanzador y empaquetado que comparten interfaces.

## Principios

- No borrar código sin conservar antes sus mediciones y ADR correspondiente.
- Cada consolidación reduce opciones activas; no las multiplica.
- Ninguna tarea de este documento debe requerir tocar el driver IddCx salvo
  el bloque 3, que trata explícitamente sobre él.
- Actualizar `WINDOWDECK_ROADMAP.md` y los ADRs cuando una decisión tomada
  aquí cambie una recomendación anterior.

## Estado de la consolidación — 10 de septiembre de 2026

| Bloque | Estado |
| --- | --- |
| 1 | Implementado: CPU recomendada, diagnósticos en `diag`, referencias históricas congeladas y guía de desarrollo. ADR 0016. |
| 2 | Pendiente: migrar el panel PowerShell y su gestión de procesos a Rust. |
| 3 | Pendiente: comparación de las rutas CPU/GPU completas en la Deck y decisión de IPC. |
| 4 | Documentación actualizada; autenticación y cifrado pendientes. |
| 5 | Distribución junto al ejecutable e integración FFI existentes documentadas en ADR 0017; instalación limpia pendiente. |
| 6 | Badges añadidos y matriz Windows/Linux comprobada en el último commit publicado. |

La referencia pública anterior a esta consolidación es `ae6fcb6`: [CI](https://github.com/ik3rurru/WindowDeck/actions/runs/34506074142)
y [Flatpak](https://github.com/ik3rurru/WindowDeck/actions/runs/34506074101) completados
correctamente. Los resultados de cada cambio posterior se comprueban en su propio
commit; una ejecución anterior no valida el código nuevo.

---

## Bloque 1 — Podar rutas experimentales de vídeo

**Motivo original:** coexistían flags de captura, vídeo y pruebas en la raíz de
la CLI. La consolidación mantiene `--driver-h264` como ruta soportada y
`--driver-native-h264` como experimento explícito. Las pruebas y referencias
históricas pasan a `diag`; la tabla de migración está en `docs/development.md`.

Tareas:

- [x] Confirmar `--driver-h264` como ruta soportada del prototipo y predeterminada
  del panel, basándose en las pruebas CPU/WGC del 8 de septiembre y de estabilidad
  del día 10. La comparación no decide todavía CPU/GPU como IPC definitivo.
- [x] Mover las pruebas puntuales y referencias históricas a `windowdeck-host diag`.
- [x] Marcar en el README qué ruta es la recomendada para un usuario nuevo y
  cuáles son solo para desarrollo.
- [x] Congelar WGC, ddagrab y RGB332 como referencias, conservando código,
  mediciones y ADR. Actualizar los scripts de prueba con los comandos nuevos.

Criterio de aceptación: un usuario que lee el README por primera vez llega a
un único comando recomendado.

---

## Bloque 2 — Migrar `WindowDeck.vbs` a un launcher en Rust

**Motivo:** Microsoft prevé deshabilitar VBScript por defecto alrededor de 2027;
su [calendario](https://techcommunity.microsoft.com/blog/windows-itpro-blog/vbscript-deprecation-timelines-and-next-steps/4148301)
no fija una retirada anticipada general para 24H2/25H2. La lógica a migrar está
en `scripts/WindowDeck.ps1`; el `.vbs` solo lo abre.

Tareas:

- Crear el crate `crates/windowdeck-launcher` (binario, no biblioteca).
- Elegir GUI mediante un prototipo de arranque, memoria, DPI y cierre. Evaluar
  `native-windows-gui` y `egui`/`eframe`; el cliente actual utiliza SDL, así que
  no se presupone una unificación de interfaz por elegir egui.
- Replicar Iniciar, Detener, Ver registros y los estados del panel PowerShell,
  incluidos elevación, errores, cancelación y recogida de procesos propios.
- Compilar como `.exe` con icono propio.
- Añadir el crate a `Cargo.toml` (`members`) y heredar
  `workspace.lints`.
- Sustituir las referencias al `.vbs` en README, `packaging/windows/README.md`,
  `scripts/package-windows.ps1` y accesos directos por el nuevo binario.
- Eliminar `WindowDeck.vbs` solo después de confirmar el reemplazo en una
  instalación limpia.

Criterio de aceptación: doble clic en el `.exe` reproduce exactamente el
panel actual de PowerShell, y `cargo clippy --workspace` cubre también el
launcher.

---

## Bloque 3 — Decidir el IPC definitivo del driver

**Motivo:** es el mayor riesgo abierto del proyecto según la propia tabla de
riesgos del roadmap original, y bloquea cualquier intento serio de reducir
latencia por debajo de lo ya medido.

Tareas:

- Cerrar la comparación A/B entre `--driver-h264` (CPU) y `--driver-native-h264`
  en la Deck. El ensayo del ADR 0012 comparaba lectura de muestras CPU, no la
  ruta GPU completa actual; registrar qué cambia en encoder y reproductor.
- Documentar la decisión en un ADR con el siguiente número libre que reemplace el estado
  "provisional" de los ADR 0011/0012.
- Si se mantiene la ruta CPU, cerrar explícitamente la rama de
  investigación D3D11 compartida en el roadmap en vez de dejarla abierta.
- Mantener el encoder fuera del IDD. Moverlo al driver sería otra decisión,
  condicionada a evidencia y justificación; no es requisito para elegir IPC.

Criterio de aceptación: existe una decisión de IPC respaldada por medidas y
recuperación en hardware real. Firma, servicio, cambio de usuario y cualquier
otra aceptación del Hito 4 todavía no comprobada permanecen explícitamente abiertas.

---

## Bloque 4 — Seguridad real, no solo documentada

**Motivo:** la Sección 8 del roadmap original describe emparejamiento y
cifrado como requisito de la v0.1, pero el flujo actual documentado en el
README describe ya la conexión TCP manual o descubierta y la ausencia actual de
autenticación y cifrado. La documentación precisa no sustituye su implementación.

Tareas:

- Diseñar el emparejamiento con código corto vinculado criptográficamente a
  la sesión, caducidad, límites de intentos y confirmación de identidad.
- Añadir cifrado autenticado a la conexión TCP completa, que hoy transporta
  control y vídeo. Documentar TLS/Noise y la transición de clientes antiguos
  sin degradación silenciosa. Autenticar antes de activar el monitor.
- Completar los límites y validación de campos en cada mensaje que ya
  llega desde red, con un test por cada caso de mensaje truncado o
  sobredimensionado (esto ya estaba previsto en el Hito 0/1 original; aquí
  se trata de confirmar que existe cobertura de test real, no solo la
  intención).
- [x] Registrar explícitamente en el README qué partes de la Sección 8 ya están
  implementadas y cuáles siguen pendientes, para no dar una falsa sensación
  de seguridad a quien lo pruebe fuera de una red de confianza.

Criterio de aceptación: control y vídeo viajan por una conexión autenticada y
cifrada; un dispositivo sin emparejar no puede activar el monitor. Hay pruebas
de rechazo de mensajes inválidos y del ciclo de emparejamiento y revocación.
El README describe las garantías implementadas y cualquier límite restante.

---

## Bloque 5 — Reducir la dependencia dura de FFmpeg externo

**Estado:** el paquete Windows ya contiene FFmpeg/FFplay y DLL, y el panel prepara
el PATH privado de sus procesos hijos. Steam Deck utiliza el runtime del Flatpak.
`windowdeck-media` ya implementa la integración FFI opcional.

Tareas:

- [x] Documentar en ADR 0017 la integración FFI existente y conservarla como
  único límite nativo, sin añadir bindings duplicados.
- [x] Distribuir FFmpeg junto al binario, con versiones, hashes y licencias.
- [ ] Validar el paquete en un Windows sin FFmpeg ni Rust previamente instalados,
  con conexión y cierre reales, y completar el instalador del Hito 7.
- No bloquear el resto del roadmap por este bloque; es optimización de
  empaquetado, no de arquitectura.

Criterio de aceptación: un usuario nuevo no necesita saber qué es FFmpeg ni
tocar variables de entorno para ejecutar el host.

---

## Bloque 6 — Visibilidad de CI

**Estado:** CI ya ejecuta formato, Clippy y pruebas en Windows/Linux, más una
matriz nativa y el workflow Flatpak. Se verificaron las ejecuciones públicas
enlazadas al principio; faltaba hacerlas visibles desde el README.

Tareas:

- [x] Añadir los badges de CI y Flatpak al README.
- [x] Confirmar ejecución en Windows y Linux, como pide el Hito 0.
- Si el `windowdeck-launcher` del Bloque 2 se añade, incluirlo en la matriz
  de CI igual que el resto de crates.

Criterio de aceptación: el criterio de aceptación del Hito 0
("`cargo fmt --check`, `cargo clippy` y `cargo test` pasan") es verificable
por cualquier visitante del repo sin clonar nada.

---

## Orden recomendado

```
Bloque 6 + estado real del 5 (documentación y distribución existentes)
→ Bloque 1 (CLI y ruta CPU soportada)
→ Bloque 2 (launcher Rust sobre la CLI consolidada)
→ Bloque 4 (seguridad, antes de invitar a nadie fuera de tu red)
→ Bloque 3 (IPC definitivo del driver, el más costoso)
→ Resto del Bloque 5 (instalación limpia e instalador del Hito 7)
```

Consolidar primero la CLI evita migrar sus flags dos veces al implementar el
launcher. Los Bloques 3 y 4 requieren validación con hardware real; se probarán
por separado para identificar con claridad el origen de una regresión.
