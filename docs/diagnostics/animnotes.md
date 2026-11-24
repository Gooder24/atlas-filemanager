• Que el dibujo ( paint ) y la gráfica ( GPU /
  present ) no tienen problema.
  • Que toda la demora pasa en el  prepaint .
  • Y que dentro de ese  prepaint , el motor de
  layout (Taffy) es el gran culpable y el agente
  estaba abriendo  ../zed-
  reference/crates/gpui/src/taffy.rs  para
  inspeccionar el motor, analizar si cachea
  correctamente las medidas ( nodes_to_measure ,
  compute_layout ) o si los splits rompen el caché y
  obligan a recalcular todo desde cero.
  Sí, confirmo al 100% que estamos en el mismo
  camino.
  Ambos detectamos que el cuello de botella central
  es Taffy haciendo recálculos locos u O(N²). Si
  queremos fluidez real de 120 fps en todo el uso
  general (especialmente al dividir vistas), el
  problema está definitivamente ahí.• Que el dibujo ( paint ) y la gráfica ( GPU /
  present ) no tienen problema.
  • Que toda la demora pasa en el  prepaint .
  • Y que dentro de ese  prepaint , el motor de
  layout (Taffy) es el gran culpable y el agente
  estaba abriendo  ../zed-
  reference/crates/gpui/src/taffy.rs  para
  inspeccionar el motor, analizar si cachea
  correctamente las medidas ( nodes_to_measure ,
  compute_layout ) o si los splits rompen el caché y
  obligan a recalcular todo desde cero.
  Sí, confirmo al 100% que estamos en el mismo
  camino.
  Ambos detectamos que el cuello de botella central
  es Taffy haciendo recálculos locos u O(N²). Si
  queremos fluidez real de 120 fps en todo el uso
  general (especialmente al dividir vistas), el
  problema está definitivamente ahí.
