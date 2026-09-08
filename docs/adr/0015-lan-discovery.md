# Descubrimiento LAN de WindowDeck

El host anuncia `_windowdeck._tcp.local.` mediante mdns-sd. El servicio contiene
el puerto TCP real y propiedades `version=1`, `codec` y `name`. Las interfaces
se revisan cada dos segundos. Una direccion de escucha explicita solo anuncia
esa direccion. La implementacion del cliente selecciona IPv4.

Sin direccion, el cliente busca H.264 durante cuatro segundos. Si encuentra un
PC conecta automaticamente; si encuentra varios muestra sus nombres para elegir
con raton, tactil o tecla numerica. Si no encuentra ninguno muestra instrucciones.
Se conserva `IP:puerto --h264-test` como alternativa manual.

La seleccion conserva el nombre completo del servicio durante la sesion; cada
reconexion consulta de nuevo sus direcciones y prueba las interfaces anunciadas.
No cambia silenciosamente a otro PC. Un cambio del nombre del equipo o del puerto
requiere volver a abrir el cliente. La identidad de servicio no es autenticacion;
se mantiene la negociacion del protocolo TCP y el alcance es una LAN de confianza.

El lanzador configura UDP 5353 entrante para el ejecutable del host, solo en redes
privadas y desde la subred local. Las redes de invitados, aislamiento de clientes
o filtros multicast pueden impedir la busqueda; en ese caso se usa IP manual.
No se instala Bonjour ni un servicio permanente. El Flatpak conserva permiso de red.

Validacion Windows: compilacion de host/cliente, formato, Clippy y 23 tests
automaticos. Ademas pasa el ensayo multicast explicito: anuncio, descubrimiento,
conexion al puerto anunciado y filtrado por identidad y codec. El ensayo de red
se excluye del CI general porque requiere multicast disponible.

Para desplegar: reiniciar el panel Windows, instalar el Flatpak generado por
GitHub y actualizar el acceso directo de la Deck con packaging/WindowDeck.desktop.
La prueba entre ambos equipos y de cambio DHCP real queda pendiente del Flatpak
nuevo; no se presenta la prueba local como validacion de ese escenario.

API de referencia: https://docs.rs/mdns-sd/latest/mdns_sd/struct.ServiceDaemon.html
