# Code signing policy

> **Status:** application to SignPath Foundation pending. Until it is approved,
> Windows releases are **not signed**, and the driver can only be installed with
> the development certificate described in
> [driver/windows-idd/README.md](driver/windows-idd/README.md).

Free code signing provided by [SignPath.io](https://about.signpath.io/),
certificate by [SignPath Foundation](https://signpath.org/).

## What is signed

Only binaries built from this repository by the
[Release](.github/workflows/release.yml) GitHub Actions workflow, never local
builds:

- `WindowDeck.exe` (control panel), `windowdeck-host.exe`,
  `windowdeck-client.exe` and `windowdeck-display.exe`, from
  `WindowDeck-VERSION-windows-x64.zip`;
- the virtual display driver, a user-mode Indirect Display Driver (IDD):
  `WindowDeckDisplay.dll` and its catalog `WindowDeckDisplay.cat`, from
  `WindowDeck-VERSION-driver-x64-unsigned.zip`.

FFmpeg, SDL2 and the other third-party libraries shipped in the package are
distributed as published by each project and are not signed with this
certificate. Their licenses are in the package's `licenses/` folder.

## Roles

| Role | Members |
| --- | --- |
| Authors (committers) | [ik3rurru](https://github.com/ik3rurru) |
| Reviewers | [ik3rurru](https://github.com/ik3rurru) |
| Approvers | [ik3rurru](https://github.com/ik3rurru) |

Single-maintainer project. External contributions are reviewed before they are
merged ([CONTRIBUTING.md](CONTRIBUTING.md)), and every release is approved
manually in SignPath before it is signed. Every account with access to the
repository or to SignPath must use two-factor authentication.

## Privacy policy

This program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or
operating it.

Specifically, WindowDeck has no telemetry, accounts or cloud services:

- While the host is running, it announces the PC on the local network over
  mDNS so the Steam Deck can find it.
- It only streams the screen to devices the user has paired by confirming a
  code on both screens, always over the local network and encrypted
  ([security](README.md#seguridad-actual)).
- Diagnostic logs stay on the device and only leave it if the user attaches
  them to an issue.

The bundled third-party components (FFmpeg, SDL2) only encode, decode and
display the video of that same connection.

## System changes

Extend mode needs the virtual display driver, which is currently installed
separately as administrator. When a session starts, the control panel asks for
administrator approval and creates firewall rules limited to private networks
and the local subnet. The virtual monitor only exists during the session.
Uninstalling the driver is documented in
[driver/windows-idd/README.md](driver/windows-idd/README.md#desinstalar-únicamente-este-prototipo).

Vulnerabilities: [SECURITY.md](SECURITY.md).
