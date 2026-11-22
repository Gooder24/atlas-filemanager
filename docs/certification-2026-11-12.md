# Atlas: correcciones del informe del 12 de noviembre de 2026

Producto: `9MX95D1GDMR3`. Partner Center muestra el envío 3, borrador
`1152921505702326297`, con el paquete `Atlas_0.1.5.0_x64.msix`.

## Dependencia de Visual C++ (10.2.4.1)

Se inspeccionaron los seis ejecutables del MSIX 0.1.5.0 con `dumpbin /dependents`.
`atlas.exe`, `atlas-search-service.exe`, `atlas-cloud-extension.exe` y
`atlas-icon-extension.exe` importan `VCRUNTIME140.dll`. El manifiesto contenido
en ese MSIX no declara ninguna dependencia de VCLibs. `rclone.exe` y `UnRAR.exe`
no muestran esa importación. Evidencia: `artifacts/certification-20261112/binary-dependencies.json`.

El manifiesto fuente ahora declara el framework de escritorio oficial
`Microsoft.VCLibs.140.00.UWPDesktop`, editor Microsoft, versión mínima
`14.0.24217.0`, según el ejemplo de Microsoft. La Store distribuye y actualiza
ese framework. No es necesario pedir al usuario que busque DLL individuales ni
añadir un descargador de vc_redist dentro de Atlas.

Este cambio todavía no modifica el MSIX subido. Para completar la corrección:

1. Crear una nueva versión (por ejemplo 0.1.6.0), con su fuente correspondiente,
   avisos y recibos. Conservar los archivos publicados de versiones anteriores.
2. Construir con `scripts/build-windows-msix.ps1 -Version 0.1.6.0 -Edition free`
   una vez preparado el snapshot de fuentes exigido por el script.
3. Inspeccionar el manifiesto del resultado y sus binarios. En una instalación
   limpia de Windows sin Visual Studio ni vc_redist, instalar el paquete y el
   framework x64 por la vía correspondiente. Para sideload, usar el paquete
   oficial de framework con `Add-AppxPackage -DependencyPath`.
4. Comprobar el arranque, búsqueda, extensiones, reinicio, actualización y
   desinstalación. Ejecutar WACK sobre el paquete exacto que se enviará.
5. Subir el nuevo paquete y explicar la dependencia en las notas privadas de
   certificación. No afirmar que estas pruebas ya se realizaron.

Alternativa: enlazar estáticamente el CRT con `-C target-feature=+crt-static`
en todos los ejecutables Rust y sus dependencias nativas. Requiere reconstruir
y verificar cada binario; no arregla bibliotecas externas precompiladas ni
elimina WinFsp. Para el MSIX actual se recomienda el framework de la Store.

## Capturas (10.1.1.3)

Reemplazar las imágenes cuestionadas por capturas reales de la versión de Atlas
que se enviará, ejecutándose en Windows. Revisar también imágenes promocionales,
tráilers y todos los idiomas. Evitar marcos de macOS, botones de ventana de otro
sistema, dispositivos ajenos a Windows y maquetas que representen funciones no
presentes. Usar archivos de muestra sin información personal. Un cambio en la
descripción no corrige este rechazo visual.

## Capacidades (10.6.3)

El informe indica que Microsoft no procesó la solicitud debido a los otros
problemas. No constituye una aprobación ni un rechazo definitivo de la
necesidad de esas capacidades. Conservar la justificación concreta de
`runFullTrust` y `unvirtualizedResources` en CERTIFICATION-NOTES.md y volver
a solicitar su revisión junto al paquete y las imágenes corregidas.

## Textos

Se reescribieron descripción, descripción corta, características y resumen de
versión de 17 idiomas fuente, que alimentan 19 fichas con las variantes de
español. Se eliminaron emojis y promesas de rendimiento o privacidad absolutos.
Se mantienen el enlace al código fuente y la información de requisitos, y se
explican los límites Free y la compra separada de Premium documentados en
`docs/build-editions.md`. Se verificaron los límites de longitud mediante
`scripts/store-listings/prepare.mjs` y la generación de las 19 fichas.

MakeAppx también validó y empaquetó correctamente una copia local del payload
0.1.5.0 con el manifiesto corregido. El log está en
`artifacts/certification-20261112/makeappx-validation.log`. El archivo
`manifest-validation-only.msix` es únicamente una prueba de empaquetado, no una
nueva versión publicable ni una prueba de instalación o de ejecución.

Las 19 fichas quedaron guardadas en el borrador del envío 3. Cada escritura
recibió HTTP 200 y se volvió a leer la ficha desde Partner Center: descripción,
descripción corta, características y notas coincidieron exactamente. Los
recibos y los valores de texto anteriores se guardaron en
`artifacts/certification-20261112/draft-receipts.json`.
No se reenviaron los cambios a certificación ni se publicó la actualización.

## Otras dependencias

WinFsp se carga de forma diferida por Atlas y es independiente de VCLibs.
El flujo opcional del controlador sigue documentado en CERTIFICATION-NOTES.md;
la corrección de Visual C++ no implica aprobación de ese flujo por Microsoft.

Fuentes oficiales:

- https://learn.microsoft.com/en-us/troubleshoot/developer/visualstudio/cpp/libraries/c-runtime-packages-desktop-bridge
- https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes
- https://learn.microsoft.com/en-us/windows/apps/publish/store-policies
