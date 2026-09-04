# 0003 — Captura inicial con Windows Graphics Capture

Estado: aceptado.

## Decisión

Usar `windows-capture` únicamente en Windows para acceder a Windows Graphics Capture y conservar cada frame como textura D3D11. El modo de diagnóstico solo valida un frame. La vista previa de integración reduce temporalmente los píxeles en CPU a 128 × 80 y RGB332 para reutilizar el transporte acotado.

## Consecuencias

Evitamos mantener código WinRT/D3D11 inseguro propio y Linux no compila esta dependencia. El reescalado y envío sin compresión no son la ruta final: se sustituirán por procesamiento en GPU y H.264 en el Hito 3. La integración se reevaluará si impide compartir directamente la textura con el encoder.
