# WindowDeck para Steam Deck

En modo escritorio, extrae todo este paquete y abre una terminal en su carpeta:

```bash
bash install-steamdeck.sh
```

El instalador instala o actualiza el Flatpak para tu usuario, crea el acceso
**WindowDeck** en tu escritorio y deja el ejecutable `~/.local/bin/windowdeck`.
No necesita sudo. La primera instalación necesita Internet para el runtime de
Flathub. Si cancelas o falla Flatpak, no crea los accesos.

Para usarlo en modo juego, abre Steam → **Añadir un producto** → **Añadir un
producto que no es de Steam** y selecciona **WindowDeck**. Si no aparece, pulsa
**Buscar**, muestra todos los archivos y selecciona
`/home/deck/.local/bin/windowdeck`. Desactiva cualquier compatibilidad forzada
con Proton: este cliente es Linux nativo. En modo juego aparecerá en **Fuera de
Steam**. Abre primero WindowDeck en el PC y pulsa **Iniciar**.

Si añades el destino manualmente, también puedes utilizar `/usr/bin/flatpak`
con estas opciones de lanzamiento:

```text
run io.github.ik3rurru.WindowDeck --fullscreen
```

Al abrir solo `WindowDeck.flatpak` en Discover se instala la aplicación y su
entrada del menú. Para crear también el acceso del escritorio, ejecuta:

```bash
bash install-steamdeck.sh --shortcuts-only
```

El escritorio se localiza con `xdg-user-dir DESKTOP`, incluso si se llama
`Escritorio` o contiene espacios. Si está desactivado, la aplicación y el
lanzador siguen disponibles sin crear un escritorio nuevo.

Para actualizar, descarga la siguiente versión y vuelve a ejecutar su
instalador. El destino de Steam conserva su ruta. Para desinstalar:

```bash
flatpak uninstall --user io.github.ik3rurru.WindowDeck
```

Después puedes borrar el acceso WindowDeck del escritorio y
`~/.local/bin/windowdeck`, y quitar su entrada de Steam. El vídeo todavía no
está cifrado: utiliza una red local de confianza.
