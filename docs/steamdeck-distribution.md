# Distribución e integración con Steam Deck

Decisión del 13 de septiembre de 2026: conservar Flatpak como paquete principal
y acompañarlo de un instalador para el escritorio y un ejecutable estable que
se pueda seleccionar desde Steam. Descargas en
[GitHub Releases](https://github.com/ik3rurru/WindowDeck/releases).

## Instalar y crear el acceso automáticamente

Extraer `WindowDeck-0.2.0-steamdeck-x86_64.tar.gz` y ejecutar en esa carpeta:

```bash
bash install-steamdeck.sh
```

El instalador ejecuta `flatpak install --user --bundle`, espera a que termine
correctamente, copia la entrada exportada por Flatpak al escritorio XDG y crea
`~/.local/bin/windowdeck` con permiso de ejecución. Puede repetirse para actualizar
sin cambiar el destino de Steam. Acepta una ruta de bundle o `--shortcuts-only`
si la aplicación ya está instalada; este último admite instalación de usuario
o del sistema. No necesita sudo.

Flatpak exporta automáticamente los archivos `.desktop` e iconos al menú del
sistema y adapta `Exec` para usar `flatpak run`. No copia la entrada al escritorio
del usuario. El `post-install` de un manifiesto corresponde a la fase de
construcción dentro del sandbox, no a una acción tras instalar en la Deck.
Por eso la creación inmediata del acceso se hace en el instalador externo.
Abrir solamente el `.flatpak` en Discover conserva la instalación convencional.
[Convenciones de Flatpak](https://docs.flatpak.org/en/latest/conventions.html#desktop-files),
[referencia del constructor](https://docs.flatpak.org/en/latest/flatpak-builder-command-reference.html).

## Añadir al modo juego

En Steam del modo escritorio: **Añadir un producto → Añadir un producto que no
es de Steam → WindowDeck**. Si no aparece en la lista, usar **Buscar**, mostrar
todos los archivos y elegir `/home/deck/.local/bin/windowdeck`. No forzar Proton.
También se puede elegir `/usr/bin/flatpak` y poner como opciones:

```text
run io.github.ik3rurru.WindowDeck --fullscreen
```

Valve documenta que las aplicaciones añadidas así aparecen en **Fuera de Steam**
en el modo juego. El script utiliza `exec`, mantiene el seguimiento del proceso
y transmite los argumentos sin reinterpretarlos. No edita `shortcuts.vdf`.
La entrada y el lanzamiento están preparados; faltan pruebas reales de vídeo,
superposición de Steam, controles y salida bajo Gamescope. La aceptación previa
en modo escritorio no demuestra esos resultados.
[Guía oficial de Valve](https://help.steampowered.com/es/faqs/view/671A-4453-E8D2-323C).

## Formatos considerados

| Formato | Uso en este proyecto | Coste o límite |
| --- | --- | --- |
| Flatpak + `.desktop` | Recomendado; runtime y bibliotecas multimedia consistentes, entrada de menú para Steam. | El bundle es un instalador, no el destino ejecutable; necesita el runtime. |
| Script ejecutable `windowdeck` | Incluido; ruta estable y pequeña para el selector de Steam, delega en Flatpak. | Necesita WindowDeck instalado; no es un paquete independiente. |
| AppImage x86_64 | Buen candidato si se exige un único archivo portátil ejecutable. | Hay que empaquetar FFmpeg/SDL, cuidar compatibilidad de bibliotecas y probar FUSE/alternativa de extracción. |
| Binario ELF con bibliotecas en `.tar.gz` | Útil para desarrollo o distribución específica para SteamOS. | Exige gestionar compatibilidad ABI, bibliotecas, iconos y actualizaciones. |
| `.exe` de Windows mediante Proton | No aporta ventajas al cliente Linux existente. | Añade otra capa de ejecución y validación. |

La recomendación de AppImage es una evaluación de empaquetado, no una build ya
validada. Su documentación describe un archivo ejecutable con dependencias
incluidas, exige comprobar los sistemas de destino y contempla problemas con
FUSE. No basta renombrar el binario Rust a `.AppImage`.
[Introducción](https://docs.appimage.org/introduction/index.html),
[pruebas](https://docs.appimage.org/packaging-guide/testing.html),
[FUSE](https://docs.appimage.org/user-guide/troubleshooting/fuse.html).

## Publicación reproducible

El workflow `release.yml`, al recibir una etiqueta `v<versión de Cargo.toml>`,
ejecuta CI en Windows/Linux, construye el Flatpak y el paquete Windows, verifica
los paquetes y publica las tres descargas con sus SHA256 como versión preliminar.
No publica hasta que los trabajos requeridos terminan correctamente. Las notas
están en `docs/releases/v<versión>.md`.

Las builds se guardan en Releases; Git conserva fuentes, scripts y manifiestos.
La release Windows exige un árbol limpio e incluye las fuentes de su commit.
Los paquetes locales y las evidencias de hardware quedan en `target/`, ignorado.
