# Code signing policy

> **Estado:** solicitud a SignPath Foundation pendiente. Hasta su aprobación,
> las releases de Windows **no están firmadas** y el driver solo se instala con
> el certificado de desarrollo descrito en
> [driver/windows-idd/README.md](driver/windows-idd/README.md).

Free code signing provided by [SignPath.io](https://about.signpath.io/),
certificate by [SignPath Foundation](https://signpath.org/).

## Qué se firma

Solo binarios compilados desde este repositorio por el workflow
[Release](.github/workflows/release.yml) de GitHub Actions, nunca builds locales:

- `WindowDeck.exe` (panel), `windowdeck-host.exe`, `windowdeck-client.exe` y
  `windowdeck-display.exe`;
- el driver de pantalla virtual: `WindowDeckDisplay.dll` y su catálogo
  `WindowDeckDisplay.cat`.

FFmpeg, SDL2 y el resto de bibliotecas de terceros incluidas en el paquete se
distribuyen tal como las publica cada proyecto y no se firman con este
certificado. Sus licencias van en la carpeta `licenses/` del paquete.

## Roles

| Rol | Miembros |
| --- | --- |
| Authors (committers) | [ik3rurru](https://github.com/ik3rurru) |
| Reviewers | [ik3rurru](https://github.com/ik3rurru) |
| Approvers | [ik3rurru](https://github.com/ik3rurru) |

Proyecto con un único mantenedor. Las contribuciones externas se revisan antes
de fusionarse ([CONTRIBUTING.md](CONTRIBUTING.md)), y cada release se aprueba a
mano en SignPath antes de firmarse. Toda cuenta con acceso al repositorio o a
SignPath debe usar autenticación en dos pasos.

## Privacy policy

This program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or
operating it.

En concreto, WindowDeck no tiene telemetría, cuentas ni servicios en la nube:

- Mientras el host está iniciado, anuncia el PC en la red local por mDNS para
  que la Deck lo encuentre.
- Solo transmite la pantalla a dispositivos que la persona ha emparejado
  confirmando un código en ambas pantallas, y siempre por la red local con
  cifrado ([seguridad](README.md#seguridad-actual)).
- Los registros de diagnóstico se guardan en el propio equipo y solo salen de él
  si la persona los adjunta a un issue.

Los componentes de terceros incluidos (FFmpeg, SDL2) solo codifican, decodifican
y muestran el vídeo de esa misma conexión.

## Cambios en el sistema

El modo Extender necesita el driver de pantalla virtual, que hoy se instala
aparte como administrador. Al iniciar una sesión, el panel pide confirmación de
administrador y crea reglas de firewall limitadas a redes privadas y la subred
local. El monitor virtual solo existe durante la sesión. La
desinstalación del driver se documenta en
[driver/windows-idd/README.md](driver/windows-idd/README.md#desinstalar-únicamente-este-prototipo).

Vulnerabilidades: [SECURITY.md](SECURITY.md).
