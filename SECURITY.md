# Seguridad

## Versiones con soporte

Solo la última release recibe correcciones de seguridad. Las versiones 0.3.x y
anteriores transmiten sin cifrar ni autenticar y no tienen soporte.

## Reportar una vulnerabilidad

No abras un issue público. Usa
[Report a vulnerability](https://github.com/ik3rurru/WindowDeck/security/advisories/new)
en la pestaña Security del repositorio, que crea un aviso privado.

Incluye la versión de ambos extremos (`windowdeck-host --version`,
`windowdeck-client --version`), los pasos para reproducirlo y su impacto.
Revisa los registros antes de adjuntarlos: contienen direcciones IP y nombres
de equipo.

WindowDeck es un proyecto personal sin plazos garantizados; la respuesta
inicial suele llegar en unos días.

## Alcance

Garantías y límites actuales: [README](README.md#seguridad-actual) y
[ADR 0022](docs/adr/0022-pairing-and-encryption.md). Quedan fuera los ataques
que requieren controlar el PC o la Deck, o ejecutar código con la cuenta del
usuario, y la información que revelan mDNS y el ritmo del tráfico.
