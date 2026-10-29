# Emparejamiento y conexión cifrada

Fecha: 29 de octubre de 2026. Estado: implementado. Probado con pruebas
automáticas en Linux y con los binarios reales del host (ruta sintética) y del
cliente en un servidor X virtual. Pendiente de aceptación en Windows con el
panel y en la Steam Deck. Rompe la compatibilidad con clientes y hosts 0.3.x.

## Motivo

La sección 8 del roadmap y el bloque 4 de la consolidación exigen que la v0.1
no acepte conexiones anónimas: emparejamiento confirmado por la persona, claves
por dispositivo, transporte cifrado y autenticado, revocación y autenticación
antes de activar el monitor. Hasta 0.3.1, cualquier equipo de la LAN podía
conectarse por TCP, activar la pantalla virtual y recibir el escritorio en claro.

## Decisión

### Transporte

- Noise `XX_25519_ChaChaPoly_SHA256` mediante `snow` 0.10, sin `ring`: solo
  crates RustCrypto (`chacha20poly1305`, `sha2`, `curve25519-dalek`) y `getrandom`.
  Noise autentica claves crudas sin certificados; XX autentica a ambos extremos
  y cifra sus claves estáticas frente a un observador pasivo.
- La conexión empieza con el preámbulo `WDKS` y la versión 1 del transporte.
  El host responde con el suyo antes de comprobar la versión, para que ambos
  extremos puedan explicar una diferencia. Los mensajes de negociación llevan
  longitud de 16 bits, un máximo de 1024 bytes y diez segundos de plazo.
- Después, todo viaja en registros: longitud de 16 bits y ChaCha20-Poly1305 con
  un nonce implícito y creciente por sentido (máximo 65 535 bytes). Un registro
  manipulado, reordenado, demasiado corto o cortado por la conexión invalida el
  canal; una escritura interrumpida invalida ese sentido. Un registro parcial
  sobrevive a los plazos de lectura y a `peek`.
- `write_message` escribe longitud y contenido en una llamada, de modo que cada
  mensaje del protocolo ocupa un registro (dos si supera 65 519 bytes).
- El protocolo 3 no cambia dentro del canal: Hello, Capabilities, la validación
  de [ADR 0020](0020-connection-validation.md) y el vídeo siguen igual.
- Medición reproducible en loopback, cifrado y descifrado incluidos, release:
  6653 Mbit/s con mensajes de vídeo de 60 kB (Xeon 2,1 GHz, cuatro núcleos), unas
  160 veces el objetivo máximo de 40 Mbps. No se ha medido en la Deck:
  `cargo test --release -p windowdeck-transport throughput -- --ignored --nocapture`.

### Identidad y dispositivos emparejados

- Cada dispositivo genera una clave X25519 por rol. Host:
  `%LOCALAPPDATA%\WindowDeck\security\host-identity.txt` y `paired-clients.txt`.
  Cliente: `$XDG_DATA_HOME/windowdeck/security/` (en Flatpak,
  `~/.var/app/io.github.ik3rurru.WindowDeck/data/windowdeck/security/`) con
  `client-identity.txt` y `paired-hosts.txt`. `WINDOWDECK_DATA_DIR` sustituye la
  carpeta en pruebas.
- En Linux se crean con permisos 0700/0600. En Windows heredan la ACL de la
  carpeta de datos del usuario. La clave privada no se cifra en reposo.
- Un archivo dañado es un error explícito; nunca se regenera en silencio, porque
  eso anularía todos los emparejamientos sin explicarlo.
- La huella son los primeros 8 bytes del SHA-256 de la clave pública
  (`1a2b-3c4d-5e6f-7a8b`). El nombre es solo una etiqueta: se reduce a una línea
  de 40 caracteres y nunca se utiliza para autorizar.

### Admisión

- El host completa la negociación y consulta la lista antes de `Hello`. Solo un
  cliente emparejado recibe la sesión; el resto se rechaza sin negociar vídeo,
  validar la red, activar el monitor ni iniciar la captura.
- El cliente solo pide una sesión a un PC cuya clave ya conoce. Si no la conoce,
  cierra tras recibirla, sin enviar su identidad ni su intención, y pide el
  emparejamiento en una conexión nueva.
- La lista se lee en cada conexión: una revocación se aplica a la siguiente
  conexión sin reiniciar el host. Una sesión en curso no se interrumpe.

### Emparejamiento

- Se pide en una conexión aparte. Si ambos extremos ya se conocen, el host lo
  confirma sin ceremonia.
- El cliente envía un compromiso: SHA-256 de una etiqueta, el resumen de la
  negociación Noise y 32 bytes aleatorios. El host responde con su propio valor
  aleatorio; el cliente revela el suyo y el host comprueba el compromiso. El
  código es el SHA-256 de una etiqueta, el resumen y ambos valores, reducido a
  seis cifras.
- Ambas pantallas muestran el código y el otro dispositivo, con su huella. Las
  dos personas deben confirmar; solo entonces cada extremo guarda la clave del otro.
- Garantía: un atacante que se interponga en la LAN mantiene dos negociaciones
  con resúmenes distintos. Como el cliente se compromete antes de conocer el
  valor del host, el atacante no puede elegir valores que igualen ambos códigos:
  acierta con probabilidad 1 entre 1 000 000 por intento, y cada intento exige
  que alguien acepte en las dos pantallas. Sin interponerse, un equipo ajeno solo
  puede pedir emparejamiento: la persona debe comprobar que su Deck muestra el código.
- La solicitud caduca a los 120 segundos. Tras tres ceremonias fallidas (rechazo,
  caducidad o error) el host rechaza nuevas solicitudes hasta Detener e Iniciar.
  El host atiende las conexiones en serie, así que hay una solicitud a la vez.
- En el PC confirma el panel: el host publica `windowdeck_pairing=ID CÓDIGO HUELLA
  NOMBRE` en su salida y lee `ID accept` o `ID reject` de `WINDOWDECK_PAIRING_FILE`;
  el identificador aleatorio descarta respuestas antiguas. Un host ejecutado en
  una terminal pregunta en la consola. Sin ninguno de los dos, rechaza la solicitud.
- En la Deck aparece una ventana táctil (también ratón, Intro y Esc). El
  reproductor integrado se cierra durante el emparejamiento y se vuelve a abrir.
  Las reconexiones automáticas nunca emparejan: el cliente termina y pide reabrirlo.

### Compatibilidad y revocación

- Sin degradación silenciosa: un cliente 0.3.x recibe un `Error` código 3 en el
  formato antiguo y el panel indica que debe actualizarse. Un cliente nuevo no
  habla en claro con un host antiguo e informa de que el PC cerró la conexión al
  iniciar el cifrado. Hay que actualizar ambos extremos; la siguiente versión
  publicada debería ser 0.4.0.
- mDNS no cambia: localiza el PC, pero la identidad la acredita su clave.
- Revocar: lista y botón del panel (con la sesión detenida), `windowdeck-host
  devices list|remove HUELLA|clear` y, en la Deck, `--paired-hosts` y
  `--forget-host HUELLA|all`.

## Alternativas descartadas

- TLS con certificados autofirmados: exige generar, fijar y renovar certificados
  para obtener lo que Noise da con una clave; `rustls` añadiría además código C
  de `ring` o `aws-lc-rs` a la compilación de Windows.
- Usar el código corto como clave precompartida: tras un solo intento activo,
  un atacante prueba fuera de línea el millón de códigos posibles.
- PAKE (SPAKE2, CPace) con el código tecleado en la Deck: misma garantía por
  intento, pero obliga a escribir con el teclado en pantalla y depende de crates
  sin auditoría. La comparación con compromiso solo pide dos toques.
- Comparar el código sin compromiso: el atacante podría probar claves efímeras
  hasta igualar ambos códigos, en torno a un millón de cálculos sin interacción.

## Comprobación

- `windowdeck-protocol`: cada tipo de mensaje rechaza cada truncamiento, los
  bytes sobrantes, longitudes mayores que el límite y campos incoherentes.
- `windowdeck-transport`: sesión bidireccional con lector y escritor
  simultáneos, escrituras de 3 MB, registros parciales tras un plazo de lectura,
  registros manipulados, cortos, cortados o reordenados, cierre desde otro hilo,
  PC desconocido, cliente no emparejado, emparejamiento aceptado, rechazado por
  cualquiera de los dos y caducado, compromiso falso, revocación, cliente y host
  antiguos, versión más nueva y mensajes de negociación inválidos.
- Host real (`diag pattern`): solo el cliente emparejado llega al protocolo; los
  rechazos no publican `negotiating`; sin panel ni terminal no se empareja; el
  panel empareja mediante su archivo; `devices remove` revoca con el host en
  marcha; tres rechazos bloquean nuevas solicitudes.
- Cliente: reconexión cifrada, cancelación, PC desconocido sin reintento y
  emparejamiento guardado solo con doble aceptación. El arnés del cliente
  integrado sigue pasando sus tres variantes a través de `secure_relay`.
- Con X virtual: ventana de código, confirmación por clic, aceptación del panel
  simulada, sesión posterior con vídeo, pantalla de rechazo y reapertura del
  reproductor integrado tras emparejar.

## Límites

- No protege frente a quien controla el PC o la Deck, ni frente a programas
  locales que lean la clave privada, que no se cifra en reposo.
- No oculta metadatos: el tamaño y el ritmo de los registros revelan actividad,
  y mDNS anuncia el PC.
- No hay rotación de claves durante la sesión; los contadores de 64 bits no se
  agotan en la práctica.
- Un equipo de la LAN puede agotar los tres intentos y bloquear el emparejamiento
  hasta reiniciar el host.
- Falta probar el panel en Windows (aviso, DPI, revocación), la ventana en la
  Deck con modo juego y el coste en la Deck. La entrada remota no existe todavía;
  cuando se implemente solo se aceptará en sesiones autenticadas.
