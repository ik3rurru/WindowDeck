# ADR 0008: primer prototipo de monitor virtual

- Estado: aceptado; escritorio activo y superficies comprobados localmente, integración con vídeo pendiente.
- Fecha: 2026-09-05

## Decisión

Adaptar el ejemplo oficial Microsoft IndirectDisplay en una capa C++ UMDF 2.25 / IddCx 1.4, separada del workspace Rust. Primera iteración: un monitor de 1280 × 800 a 60 Hz, identidad estable y una utilidad `SwDeviceCreate` que conserva el dispositivo hasta cerrar su handle.

Se reutilizan WDF para la vida de los contextos, D3D11 para consumir superficies y PnPUtil para instalación/desinstalación. SDK y WDK se fijan por NuGet; el build produce un paquete sin firmar y una autoprueba sin efectos sobre los dispositivos.

Actualización 2026-09-06: seleccionar explícitamente una GPU física de bajo consumo antes de anunciar el monitor mediante [IddCxAdapterSetRenderAdapter](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/iddcx/nf-iddcx-iddcxadaptersetrenderadapter). DXGI ordena por preferencia y D3DKMT excluye adaptadores indirectos, de software o sin capacidad de renderizado. La constante `RenderPreference` permite ajustar esta elección al compilar. Si la selección falla, se registra el fallo por salida de depuración y se conserva la elección de Windows. Es una solución comprobada para este entorno: AMD integrada activa el escritorio; la NVIDIA física no lo hace. No se ha identificado el motivo interno de esa diferencia ni validado otros equipos.

El worker informa de las estadísticas de cada frame descartado, con su número y hora real de adquisición y cero bytes transmitidos. `--verify` consulta actividad, resolución y frecuencia sin cambiar la configuración; `--probe` realiza esa comprobación antes del cierre normal. Los diez ciclos ya verifican el escritorio activo además de PnP.

## Consecuencias

No se introduce IPC, servicio residente ni integración con H.264 antes de validar la enumeración, extensión y retirada del monitor. El worker descarta las superficies tras notificarlas a Windows. La calidad parametrizable y el ajuste de latencia quedan aplazados por decisión del usuario; no se dan por cumplidos los criterios pendientes del hito 3.

La firma de pruebas y la confianza del certificado se realizan mediante un script separado, con autorización explícita y privilegios de administrador. No se cambiaron Secure Boot/TESTSIGNING; el usuario reinició manualmente durante el diagnóstico. La enumeración PnP no basta para aceptar el prototipo: se comprobaron también escritorio activo y superficies, pero siguen pendientes vídeo en la Deck y los escenarios de suspensión/bloqueo. El ejemplo original está bajo MS-PL; se conserva esa licencia en `driver/windows-idd/`, sin cambiar la licencia de los crates Rust.
