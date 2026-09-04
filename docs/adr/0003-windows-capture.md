# 0003 — Captura inicial con Windows Graphics Capture

Estado: aceptado.

## Decisión

Usar `windows-capture` únicamente en Windows para acceder a Windows Graphics Capture y conservar cada frame como textura D3D11. El primer incremento solo selecciona un monitor y valida un frame; todavía no copia ni envía sus píxeles.

## Consecuencias

Evitamos mantener código WinRT/D3D11 inseguro propio y Linux no compila esta dependencia. La integración se reevaluará si impide compartir directamente la textura con el futuro encoder.
