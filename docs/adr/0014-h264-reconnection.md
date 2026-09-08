# ADR 0014 — Reconexión del cliente H.264

Fecha: 2026-09-08. Estado: implementado; validado localmente, pendiente de repetir suspensión en la Deck.

## Problema

La Deck suspende la red y sus procesos. El host termina la sesión y retira el monitor por timeout. El cliente H.264 anterior salía al despertar por un error de lectura; el bucle de reconexión RGB332 no se aplicaba a este modo.

## Decisión

Después de una primera conexión correcta, el worker H.264 reintenta los fallos de transporte con una espera de un segundo entre intentos. Cada conexión negocia nuevas capacidades e identificador de sesión y reinicia los contadores de chunks. Se cierra el socket viejo antes de reintentar. Errores de protocolo, configuración incompatible, mensajes Error y Stop explícito terminan la sesión; no generan reintentos. El arranque inicial conserva el comportamiento anterior si el host no está disponible.

FFplay y su pipe de entrada se conservan durante los cortes. La imagen puede quedar congelada mientras vuelve la red; mantener el pipe abierto impide que `-autoexit` cierre el reproductor por un EOF creado por el propio worker. Al regresar el vídeo se procesa el MPEG-TS de la nueva sesión, sin cambiar calidad ni opciones de buffering.

El hilo principal observa la salida de FFplay. Cerrar la X cancela los reintentos y cierra el socket activo. La publicación del nuevo socket se coordina mediante mutex y una comprobación de cancelación, para que un cierre concurrente no deje otra conexión viva. La espera entre reintentos consulta cancelación cada 20 ms; un intento de conexión/handshake en curso mantiene los timeouts existentes (3 s de conexión y 10 s por operación de socket).

## Validación

Tres tests nuevos cubren EOF seguido de nueva negociación y reinicio de secuencias, cancelación durante la espera sin nueva conexión y separación entre fallos de transporte/protocolo. Pasan 22 tests del workspace Windows, formato y Clippy.

La prueba local `target/test-h264-reconnect.py` valida dos segmentos MPEG-TS con timestamps reiniciados y una pausa, usando FFplay con SDL dummy: el filtro showinfo confirma frames rojos y azules. Después ejecuta el cliente contra dos sesiones TCP y verifica un solo reproductor, una reconexión, primeros paquetes de ambas sesiones y salida ante Stop. Evidencia correcta: `target/h264-reconnect-1788857803242749000/`. El primer ensayo tenía un identificador de códec equivocado en el servidor sintético y fue corregido; no fue un fallo del cliente.

Pendiente verificar suspensión real, reproducción al despertar y cierre interactivo con el Flatpak actualizado. No se promete mantener viva la sesión TCP durante suspensión: se crea una nueva sesión al recuperar la conectividad.
