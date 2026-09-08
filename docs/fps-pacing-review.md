# CPU frame pacing measurement

The CPU streaming helper now uses QueryPerformanceCounter and a high-resolution
waitable timer at fixed 60 Hz deadlines. Expired deadlines are skipped without
catch-up bursts. Its native self-test checks deadline progression and skipped slots.

`driver_cpu_interval` reports interval output FPS, mean/max pipe write duration
in microseconds and missed deadlines. FFmpeg progress additionally reports
`interval_fps`; its existing `fps` field remains the cumulative average.
Neither metric measures frames displayed on the Deck.

Validation: native build and self-test, 22 Rust tests and Clippy passed.
Live evidence: `target/deck-cpu-live-611371701eef48a89fb6518ee6b2f95a/host.log`.
The initial steady interval reached 60.046 helper FPS with zero missed deadlines.
Later writes blocked up to 640825 us and output fell sharply. This proves
downstream backpressure reaches the helper; it does not isolate encoding,
transport or playback as its source. Further controlled measurements are needed.

This change affects the helper executable and host, not the installed driver.
Previous comparisons of driver changes based only on rebuilding and restarting
the helper do not validate those changes. The signed package and installation log
were dated September 7, while subsequent rebuilt DLLs were dated September 8.
Do not infer performance changes from those sessions or compare cumulative FPS
from sessions with different startup durations and content.

The checkpoint includes the previously unpublished native dependencies and fixes
the literal PowerShell newline escapes in the published FramePublisher source.
Rust CI alone does not compile the native driver; the native build was validated locally.

## Cierre acordado con el usuario — 8 de septiembre de 2026

El usuario confirma que las caídas de FPS se perciben al activar los scripts e
iniciar la comunicación, pero que durante el uso normal observa una transmisión
estable. Acepta el comportamiento actual y pide pausar la optimización de FPS.
Esta aceptación es perceptiva: no certifica 60 frames presentados por segundo
de forma continua. No se investigarán ahora los bloqueos iniciales ni se iniciará
la integración de un encoder GPU.

Se conservan la ruta CPU con libx264, la temporización de alta resolución y las
métricas por intervalo. No hace falta otro Flatpak para estos cambios del host.
La última sesión se dejó activa para el usuario; antes de reutilizar sus PID,
comprobar sus identidades. El directorio indicado arriba contiene los PID y logs.

Al retomar, usar contenido y duración comparables, separar calentamiento de uso
estable y comprobar las versiones de auxiliar, host, Flatpak y driver instalado.
Medir bloqueos de escritura en cada etapa antes de atribuir las caídas al encoder.
Las optimizaciones anteriores del driver siguen sin una comparación válida de
rendimiento con el paquete efectivamente instalado.
