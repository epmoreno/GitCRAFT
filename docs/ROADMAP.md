# GitCRAFT — Roadmap

Cada tarea es **verificable**: tiene un criterio de aceptación concreto. Una tarea
está terminada cuando compila, pasa `npm run check` (lint + tests de Rust + tests de
frontend) y está en un commit.

Leyenda: `[ ]` pendiente · `[~]` en curso · `[x]` hecha

---

## Fase 0 — Cimientos

- [x] **0.1 Documentación base.** Existen `docs/VISION.md`, `docs/ROADMAP.md`,
      `CLAUDE.md` y `LICENSE` (MIT).
- [x] **0.2 Arquitectura aprobada.** `docs/ARCHITECTURE.md` describe los módulos
      y el flujo UI → comando → proceso `git` → evento → UI.
- [x] **0.3 Esqueleto Tauri 2 + React + TS.** `npm run tauri dev` abre una
      ventana vacía con el layout de tres zonas. CI local con `npm run check`
      (clippy, rustfmt, cargo test, eslint, tsc, vitest).
- [x] **0.4 Ejecutor de git.** Un módulo Rust lanza `git` con argumentos en array,
      un entorno controlado (`LC_ALL=C`, `GIT_TERMINAL_PROMPT=0`…), timeout,
      cancelación y streaming de stdout. Tests: ejecutar `git --version`,
      cancelar un proceso largo y comprobar que no se usa shell.
- [x] **0.5 Fixtures de test.** Un helper crea repos temporales con historial
      sintético (ramas, merges, tags) para los tests de integración.

## Fase 1 — MVP

Orden acordado tras la fase 4: 5.2 (ramas) → 7.x (stash) → 6.x (remotos) →
1.3 (pestañas). Para cerrar el MVP: T.3 → 5.1 (filtro) → 1.4 → 1.5 → T.1 →
T.2 → T.4.

### 1. Repositorios
- [x] **1.1 Abrir repo.** Diálogo nativo → validación con
      `git rev-parse --show-toplevel` → aparece la pestaña. Abrir una carpeta que
      no es un repo da un error claro. *Test:* integración con repo y no-repo.
- [x] **1.2 Recientes.** La lista persiste entre sesiones (máx. 20) y quita
      entradas de rutas que ya no existen.
- [x] **1.3 Pestañas.** Varias pestañas con estado independiente (selección,
      scroll, panel abierto); se pueden cerrar y reordenar arrastrando; se
      restauran al reiniciar con la pestaña activa. Al restaurar, un repo que
      ya no existe o no es válido no se abre: se avisa una vez y se quita de
      la sesión guardada. Atajos: ⌘W cerrar, ⌘1…⌘9 ir a la pestaña, ⌃Tab /
      ⌃⇧Tab siguiente y anterior.
      *Estado:* reordenar arrastrando la pestaña o con su menú contextual
      (mover a la izquierda/derecha, cerrar). Cada pestaña recuerda el scroll
      del historial, del panel de detalle, del diff y de la barra lateral, y
      las secciones plegadas; la selección, el diff abierto y el borrador del
      commit ya eran por repositorio. La sesión (rutas en orden y la activa)
      se guarda en `settings.json` en cada cambio y se restaura al arrancar
      sin reordenar los recientes. ⌘W está en el menú de la app («Ventana →
      Cerrar pestaña»), que sustituye al de Tauri, cuyo ⌘W cerraba la
      ventana. Los atajos no actúan con un diálogo abierto.
      *Tests:* integración en el backend (restaurar en orden con la activa,
      repos borrados o no válidos avisados y olvidados, recientes intactos) e
      interfaz (guardar la sesión, mover, ciclar, arrastrar, menú de la
      pestaña, atajos, aviso de restauración, scroll por pestaña).
      *Verificado manualmente:* varias pestañas con ruta y atajo en el tooltip;
      estado por pestaña (scroll, selección, diff, secciones plegadas);
      reordenar arrastrando y con el menú contextual; atajos ⌘1–3, ⌃Tab/⌃⇧Tab,
      ⌘W cierra solo la pestaña y bloqueados con un diálogo abierto; menú de
      la app y copiar/pegar; restaurar orden y pestaña activa tras ⌘Q; repo
      movido: aviso una sola vez y pestaña no abierta.
- [x] **1.4 Inicializar repo.** `git init` con rama inicial configurable.
      *Estado:* «Inicializar repositorio…» en la página de inicio y en el menú
      ▾ junto a «Abrir repositorio». Tras elegir la carpeta, un diálogo
      propone la rama inicial (`init.defaultBranch` o «main»), la valida al
      escribir, avisa si la carpeta está dentro de otro repositorio (quedaría
      anidado) y de que los archivos existentes no se tocan. Si ya es un
      repositorio, no lo reinicializa: ofrece abrirlo. El nuevo se abre en una
      pestaña.
      *Tests:* integración (rama elegida, archivos existentes sin tocar, ya es
      un repositorio, anidado, nombres no válidos, carpeta inexistente) y de
      interfaz.
      *Verificado manualmente:* rama «trunk», nombre no válido, carpeta con
      archivos, aviso de anidado y carpeta que ya es un repositorio.
- [x] **1.5 Clonar.** URL + destino, progreso (`--progress`) y cancelación; la
      autenticación la gestiona el credential helper. *Test:* clonar un repo
      local `file://`.
      *Estado:* «Clonar repositorio…» en la página de inicio y en el menú ▾
      junto a «Abrir repositorio». URL, carpeta (la última usada o la de
      inicio) y nombre (se deduce de la URL hasta que lo editas). El destino
      se comprueba al escribir: si existe y no está vacío, se niega y se
      explica; una carpeta vacía sí vale. Progreso de git y «Cancelar
      clonación»; Esc no cierra el diálogo mientras clona. El clon se abre en
      una pestaña. Las URLs que empiezan por «-» se rechazan.
      *Tests:* integración solo con repos locales (`file://` y rutas):
      clonar con upstream y ramas remotas, carpeta vacía, nunca encima de una
      carpeta con archivos, nombres y URLs no válidos, remoto inexistente,
      cancelación; e interfaz.
      *Verificado manualmente:* clon normal con upstream, destino con
      contenido bloqueado, carpeta vacía y remoto inexistente.

### 2. Grafo de commits
- [x] **2.1 Parser de log.** `git log --topo-order -z --format=<campos separados
      por %x00/%x1f>` se parsea a `Commit { oid, parents, author, … }`. *Tests
      unitarios:* mensajes con saltos de línea, UTF-8, commits raíz, merges con
      más de 2 padres y firmas vacías.
- [x] **2.2 Refs.** `git for-each-ref` produce ramas, remotos, tags y HEAD (también
      HEAD separado). *Tests unitarios + integración.*
- [x] **2.3 Layout de carriles.** Un algoritmo en Rust asigna una columna y un
      color a cada commit, con segmentos de arista por fila. *Tests:* historial
      lineal, rama + merge, octopus, ramas cruzadas; el layout es estable con la
      carga incremental.
- [x] **2.4 Carga incremental.** Páginas de N commits en streaming; el layout
      continúa sin recalcular lo anterior. *Benchmark:* 100k commits; la primera
      página en < 150 ms y el total < 3 s en M1 (se registran los resultados en
      `docs/benchmarks.md`). *Estado:* el total cumple siempre. La primera
      página cumple con `commit-graph` (83 ms). En repos de más de 10.000
      commits sin él, GitCRAFT ofrece «Optimizar repositorio» (nunca lo genera
      sin permiso; el aviso se puede descartar por repo).
      *Verificado manualmente* con un repo de 12.000 commits sin commit-graph:
      el aviso aparece, optimiza con progreso, no vuelve tras optimizar, y «No
      mostrar más» se recuerda tras reiniciar.
- [x] **2.5 Render virtualizado.** Canvas para el grafo y filas virtualizadas
      para el texto; scroll a 60 fps con 50k commits. Muestra etiquetas de
      ramas, tags y HEAD.
      *Estado:* scroll fluido, verificado manualmente con un repo grande.
- [x] **2.6 Refresco.** Un watcher de `.git` (con debounce) refresca refs y el
      grafo tras cambios externos (por ejemplo, un commit hecho en la terminal).
      *Estado:* vigila HEAD, `packed-refs` y `refs/` del git dir (y del común en
      worktrees) con FSEvents; debounce de 300 ms; las operaciones propias se
      silencian. La UI mantiene las filas y la selección hasta tener el grafo nuevo.

### 3. Detalle de commit
- [x] **3.1 Metadatos.** Autor, committer, fechas, mensaje completo, padres
      navegables y refs.
      *Verificado manualmente:* el hash completo se muestra en una línea,
      recortado con «…»; el reajuste del cuerpo del mensaje.
- [x] **3.2 Archivos cambiados.** `git diff-tree -r -z --name-status -M` con
      estado A/M/D/R/C y contadores +/−. *Tests:* renombrados, binarios y commit
      raíz.
- [x] **3.3 Diff.** Parser de diff unificado (hunks, líneas, no-newline-at-EOF,
      binarios) y vista con números de línea y resaltado. Los archivos grandes
      se cargan bajo demanda.

### 4. Working directory
- [x] **4.1 Estado.** `git status --porcelain=v2 -z --branch` se parsea a
      unstaged, staged, untracked y conflictos, además de ahead/behind. *Tests
      unitarios.*
      *Verificado manualmente* en un repo de prueba: «Cambios sin confirmar»
      con contadores y actualización al editar desde la terminal.
- [x] **4.2 Stage/unstage por archivo** (también todos a la vez).
      *Verificado manualmente* en un repo de prueba: Preparar/Quitar por
      archivo y doble clic.
- [x] **4.3 Stage/unstage por hunk y por línea.** Se genera un parche y se aplica
      con `git apply --cached` (con `-R` para unstage). *Tests:* hunks
      parciales, líneas sueltas y finales de archivo.
      *Verificado manualmente* en un repo de prueba: stage por hunk y por
      línea.
- [x] **4.4 Descartar cambios** con confirmación.
      Por archivo, hunk y línea. Copia previa en el stash y en blobs; los
      archivos sin seguimiento van a la Papelera; «Deshacer» durante 10 s.
      *Tests:* descartar + deshacer deja el contenido idéntico byte a byte;
      recuperar desde el stash un descarte parcial de un archivo con cambios
      preparados, sin tocar el índice.
      *Verificado manualmente* en un repo de prueba: descartar una línea y
      «Deshacer» antes de 10 s; un archivo sin seguimiento a la Papelera y
      recuperado; «Deshacer» se niega si el archivo se edita antes.
      *Verificado manualmente* también la recuperación tardía (pasado el
      aviso) desde la app con «Recuperar» (7.3), sin diálogo y con
      confirmación, con el índice intacto. Desde la terminal, por archivo:
      `git restore --source=stash@{N} --worktree -- <ruta>` (nunca
      `git stash apply`, que aborta si el archivo tiene más cambios).
- [x] **4.5 Commit.** Resumen + descripción, contador de caracteres; amend
      precarga el último mensaje. Respeta hooks (`pre-commit`, `commit-msg`) y
      muestra su salida si fallan.
      *Verificado manualmente* en un repo de prueba: commit con resumen y
      descripción (coincide con `git log --oneline`) y amend con el mensaje
      precargado.

### 5. Ramas
- [x] **5.1 Barra lateral.** Ramas locales, remotas (agrupadas por remoto), tags y
      stashes, con filtro.
      *Estado:* ramas locales, remotos, tags y stashes (clic → commit en el
      grafo). Filtro arriba de la barra lateral, sin distinguir mayúsculas:
      ramas (también por su upstream), ramas remotas (el nombre del remoto
      muestra todas las suyas), tags y stashes (mensaje y rama). Cada sección
      muestra «visibles/total» y se abre mientras se filtra; Esc borra el
      filtro. Se recuerda por pestaña.
      *Verificado manualmente:* filtrar por «feat», por «origin» y stashes;
      secciones plegadas; Esc; filtro por pestaña.
- [x] **5.2 Crear, checkout, renombrar y borrar.** Borrar una rama no fusionada
      pide confirmación y explica qué commits quedarían sin rama; nunca se usa
      `branch -D` sin esa confirmación. Cambiar de rama con cambios sin
      confirmar que chocan: se explica el conflicto y se ofrece «Guardar en el
      stash y cambiar» o «Cancelar»; nunca se fuerza el checkout. Para hacer
      checkout de una rama remota se crea una rama de seguimiento.
      *Estado:* barra lateral con doble clic (cambiar), menú contextual
      (cambiar, nueva rama desde aquí, renombrar, borrar) y botón «+»; botón
      «Rama» de la barra superior. Borrar solo si la rama sigue donde se
      confirmó; el diálogo muestra el comando para volver a crearla.
      *Tests:* integración (nombres, crear con seguimiento, renombrar la
      actual, commits sin rama, borrado con rama movida, cambio bloqueado sin
      tocar nada, stash y cambiar byte a byte) y de interfaz.
      *Verificado manualmente* en un repo de prueba, al completo: cambiar de
      rama (doble clic y menú contextual); cambio bloqueado («Cancelar» no
      toca nada, «Guardar en el stash y cambiar» funciona); crear (rechaza
      nombres inválidos y repetidos); renombrar; borrar (avisa de los commits
      sin rama y muestra cómo recuperarla) y no borrar una rama que cambió con
      el diálogo abierto.

### 6. Remotos
Las pruebas usan un repo bare local como remoto, nunca un servidor real:
`scripts/make-remote-sandbox.sh` crea desde cero `~/gitcraft-remoto-prueba`
(repo de trabajo), `~/gitcraft-remoto.git` (remoto bare) y un segundo clon que
simula a «otra persona» que hace push. Credenciales: solo el credential helper
de git, con `GIT_TERMINAL_PROMPT=0`; un fallo de autenticación se explica con
un mensaje claro. Fetch, pull y push muestran progreso y se pueden cancelar.

- [x] **6.1 Fetch** (todos los remotos o uno, `--prune` opcional) con progreso y
      cancelación.
      *Estado:* botón «Fetch» (todos los remotos) con menú ▾ (con `--prune`
      o un remoto concreto) y menú contextual en cada remoto de la barra
      lateral. Panel de progreso con la línea de git y «Cancelar». Errores de
      autenticación y de conexión con mensajes propios. Script
      `scripts/make-remote-sandbox.sh` (con `otra-push` para simular a otra
      persona).
      *Tests:* integración (fetch sin tocar ramas locales, `--prune` solo si
      se pide, remotos desconocidos o inalcanzables, cancelación) y de
      interfaz.
- [x] **6.2 Pull** con modo merge o rebase configurable por repo (por defecto,
      lo que diga `pull.rebase`). Si hay cambios sin confirmar que lo impiden o
      conflictos, se explica el motivo y se ofrecen alternativas; nunca se
      fuerza.
      *Estado:* botón «Pull» con menú ▾ (merge o rebase para este repositorio,
      o lo que diga la configuración de Git). Primero hace fetch del remoto
      del upstream; antes de tocar nada comprueba los cambios sin confirmar
      que chocarían (con rebase, cualquiera en archivos con seguimiento) y
      predice los conflictos con `git merge-tree`. Si algo estorba: «Guardar
      mis cambios y hacer pull» (stash que nunca se borra) o cancelar. Si
      habrá conflictos: «Hacer pull con conflictos» o cancelar. Un merge o
      rebase a medias se muestra en un aviso con «Abortar…» (con
      confirmación). Sin upstream, upstream borrado y HEAD separado se
      explican.
      *Tests:* integración (fast-forward, merge, rebase, bloqueos por cambios
      y por archivos sin seguimiento, conflictos previstos con merge y rebase,
      abortar, guardar y hacer pull, sin upstream, upstream borrado,
      configuración de Git) y de interfaz.
- [x] **6.3 Push** y push con `--set-upstream`: si la rama no tiene upstream,
      se ofrece crearlo. Si el remoto tiene commits nuevos (non fast-forward)
      se explica que hay que hacer pull primero. El force push usa solo
      `--force-with-lease` y pide confirmación listando los commits del remoto
      que se sobrescribirían.
      *Estado:* botón «Push» con menú ▾ («Push forzado (con lease)…»). Sin
      upstream: diálogo para publicar la rama en un remoto y crearlo. Rechazos
      leídos de `git push --porcelain`: si el remoto tiene commits nuevos se
      explica y se ofrece «Hacer pull». El push forzado hace antes fetch,
      lista los commits del remoto que se perderían y usa
      `--force-with-lease=<rama>:<commit mostrado>`: si el remoto se movió
      después, se rechaza y se explica.
      *Tests:* integración (push, al día, publicar con upstream, remoto por
      delante, lease caducado, forzado con lease, fixture de
      `push --porcelain`) y de interfaz.
- [x] **6.4 Indicadores ahead/behind** en la rama (barra superior) y en la
      barra lateral.
      *Estado:* la barra superior muestra la rama actual con «↑N ↓M» (o «sin
      upstream» / «borrada en el remoto»), con la explicación completa al pasar
      el ratón; cada rama local de la barra lateral, igual junto a su
      upstream. Se cuentan con `rev-list --left-right --count` contra el
      upstream (hasta 64 ramas) cada vez que se leen las refs, así que se
      actualizan tras fetch, commits y cambios hechos desde la terminal.
      *Tests:* integración (por delante, por detrás tras fetch, upstream
      borrado) y de interfaz.

*Verificado manualmente* la 6.x al completo con
`scripts/make-remote-sandbox.sh`: indicadores y fetch (↓1 tras el fetch);
pull limpio (avance rápido); pull bloqueado («Cancelar» / «Guardar mis cambios
y hacer pull»); conflictos y abortar, con merge y con rebase; push rechazado
por commits nuevos en el remoto; push forzado (lease caducado rechazado y
forzado real); publicar una rama sin upstream; prune (rama «borrada en el
remoto»); remoto inalcanzable.

### 7. Stash
Nunca se borran stashes automáticamente; borrar uno siempre pide
confirmación.

- [x] **7.1 Lista, guardar, aplicar, pop y borrar.** La barra lateral muestra
      todas las entradas con mensaje, rama de origen y fecha; las creadas por
      GitCRAFT («descarte de …», «antes de cambiar a …») se distinguen. Guardar
      desde el botón «Stash» con mensaje opcional y opción de incluir archivos
      sin seguimiento. Aplicar y pop nunca fuerzan: si hay conflictos o cambios
      que se sobrescribirían se explica y se ofrecen alternativas; pop solo
      borra la entrada si se aplicó sin conflictos. Borrar siempre con
      confirmación, mostrando el hash para recuperarla
      (`git stash store <hash>`).
      *Estado:* sección «Stashes» de la barra lateral (etiquetas «descarte» y
      «cambio de rama»; rama y fecha), menú contextual (aplicar, pop, borrar),
      botón «Stash» de la barra superior. Si algo estorba, «Guardar mis
      cambios y aplicar»; si habrá conflictos (previstos con
      `git merge-tree`), «Aplicar con conflictos» o cancelar.
      *Tests:* integración (fixture de `stash list`, crear, aplicar y pop por
      id, bloqueo sin tocar nada, conflictos previstos, borrar y recuperar con
      `stash store`) y de interfaz.
- [x] **7.2 Ver contenido** de un stash en el panel de detalle, con diff por
      archivo (incluidos los archivos sin seguimiento guardados con `-u`).
      *Estado:* al pulsar una entrada, el panel muestra mensaje, origen, rama,
      fecha, `stash@{n}` y hash, los botones aplicar/pop/borrar y dos listas:
      cambios (contra `<stash>^1`) y archivos sin seguimiento (el commit
      `<stash>^3`); cada archivo abre su diff.
- [x] **7.3 Recuperar archivo** en las entradas «GitCRAFT: descarte de …»:
      `git restore --source=<stash> --worktree -- <ruta>`. Si el archivo cambió
      desde el descarte, pide confirmación mostrando qué se sobrescribiría.
      *Estado:* botón «Recuperar» por archivo en el panel de la entrada. Si
      recuperar solo devuelve lo descartado, se hace directamente; si el
      archivo tiene líneas que la copia no tiene (trabajo nuevo), se listan y
      se pide confirmación; si no se puede comparar línea a línea (binario o
      sin seguimiento), se avisa de que se sustituye entero. Solo se aplica si
      el archivo sigue como en esa comprobación. La entrada no se borra.
      *Tests:* integración (el caso pendiente de la 4.4 —descarte de una línea
      en un archivo con cambios preparados— recuperado byte a byte sin tocar
      el índice; trabajo nuevo listado; archivo cambiado tras la comprobación;
      archivo borrado) y de interfaz.

*Verificado manualmente* la 7.x al completo en un repo de prueba: lista,
etiquetas y refresco desde la terminal; crear un stash desde la app (con
archivos sin seguimiento) y ver su contenido; pop limpio; cambios que se
sobrescribirían («Cancelar» / «Guardar mis cambios y aplicar»); conflictos
(«Cancelar» / «Aplicar con conflictos», entrada conservada); borrar y
recuperar con `git stash store`; recuperación tardía de un descarte, sin
diálogo y con confirmación, con el índice intacto.

### Transversal del MVP
- [x] **T.1 Errores.** Un catálogo traduce los errores conocidos de git a
      mensajes claros, con la salida original desplegable.
      *Estado:* inventario y catálogo en ARCHITECTURE.md (sección 6, «Errores
      hacia la UI»). Todos los errores se muestran con `ErrorReport`: qué se
      intentaba, explicación, salida original desplegable y «Copiar la
      salida». Nuevos tipos: `remoteNotFound`, `indexLocked`,
      `localChangesOverwritten`, `unrelatedHistories`, `notFastForward`,
      `diskFull`, `filePermission`. Dejan de ser silenciosos: activar amend,
      leer la sesión anterior, guardar el modo de pull, comprobar el destino
      del clon y push sin remotos.
      *Verificado manualmente:* aviso con salida desplegable y «Copiar la
      salida»; `index.lock` bloqueado; credenciales redactadas en el error del
      clon; amend sin commits y push sin remoto.
- [x] **T.2 Panel de operaciones.** Muestra las operaciones en curso con progreso
      y botón de cancelar, y un historial de comandos ejecutados.
      *Estado:* botón «Operaciones» → panel inferior. El historial muestra
      el comando exacto, su duración y su resultado, con las credenciales de
      las URLs redactadas (`***`). Detalles en ARCHITECTURE.md (sección 6).
      *Verificado manualmente:* comando exacto y duración, etiqueta
      «escritura», filtros y limpiar historial; URL redactada en el
      historial.
- [x] **T.3 i18n.** Todos los textos de la UI están en `es` y `en`; hay un test
      que falla si falta una clave.
      *Estado:* `src/i18n/i18n.test.ts` (desde la 0.3) falla si una clave
      está en un idioma y no en el otro, o si un texto está vacío. Se añade,
      sin duplicarlo, lo que no cubría: cualquier test falla si un componente
      usa una clave que no existe (`missingKeyHandler` en
      `src/test/setup.ts`), así que una clave mal escrita no llega a mostrarse
      como texto crudo.
- [x] **T.4 Empaquetado.** `.dmg` para Apple Silicon (sin firmar en el MVP).
      *Estado:* `npm run dist` genera `GitCRAFT.app` y
      `GitCRAFT_<versión>_aarch64.dmg` (arm64, firma ad-hoc sin identidad, sin
      notarizar, macOS ≥ 11). Instalar, primer arranque con Gatekeeper sin
      desactivarlo, y desinstalar: `docs/INSTALACION.md`. Detalles en
      ARCHITECTURE.md (sección 6, «Empaquetado»).
      *Verificado manualmente:* `.dmg` instalado en Aplicaciones, arranca
      desde Finder sin problemas, y Fetch y Push funcionan desde la app
      instalada.

**MVP (fase 1) completo y verificado manualmente** — versión `v0.1.0`.

## Fase 1.5 — Pulido, ajustes y GitHub (v0.2.0)

Orden: A → B → C → D → E. Al terminar cada letra se para y se explica cómo
probarla antes de seguir. Identidad visual propia (grafito/cobre); los
comportamientos se describen con palabras propias, sin copiar el aspecto de
ningún producto.

### A. Estética y contraste
- [x] **A.1 Paleta revisada y test de contraste.** Los colores de
      `src/ui/tokens.css` se revisan para que sean más suaves (acento y
      colores de estado menos saturados) y mantienen la identidad
      grafito/cobre. Todo color de la app sale de un token: no queda ningún
      color literal en los CSS de componentes ni en el código (los carriles
      del grafo incluidos).
      *Tests:* `src/ui/contrast.test.ts` lee los tokens y calcula el ratio
      WCAG de los pares principales. Falla si el texto (`--text`,
      `--text-muted`, acento, peligro, éxito, aviso e info, y el texto sobre
      el acento) baja de **4,5:1** sobre cualquier superficie en la que se
      usa, o si el anillo de foco y los bordes de los controles bajan de
      **3:1**. Otro test falla si aparece un color literal fuera de
      `tokens.css`. La tabla de pares y ratios se documenta en
      ARCHITECTURE.md.
      *Estado:* acento, estados y carriles menos saturados, y texto
      secundario más claro (`--text-faint` pasa de 3,9:1 a ≥ 4,6:1).
      - Nuevos tokens derivados: selección, tintes, diff, avisos, sombras y
        `--border-control` para el contorno de los controles.
      - Los carriles del grafo pasan a ser `--lane-1` … `--lane-8`.
      - El texto seleccionado se pinta siempre en `--text`.
      *Verificado manualmente:* el contraste de los textos tenues (hashes,
      fechas, contadores y rutas).
- [x] **A.2 Estados de interacción.** Tokens y estilos base comunes para
      *hover*, *pulsado*, *seleccionado* (con y sin foco en la lista), *foco
      de teclado* y *deshabilitado*. Se aplican a botones, filas de listas,
      pestañas, chips, elementos de menú, campos y casillas. Siempre se ve
      qué elemento está activo o se está pulsando. Lo deshabilitado se
      distingue sin depender solo del color y no reacciona al *hover*.
      *Tests:* los pares de color de cada estado entran en el test de
      contraste. Un test de componentes comprueba que los controles
      deshabilitados llevan `disabled`/`aria-disabled` y que la selección se
      expone con `aria-selected`/`aria-current`.
      *Estado:* reglas comunes en `tokens.css`; detalles en ARCHITECTURE.md
      («Estados de interacción»).
      - *Hover* (aclara) y pulsado (oscurece) son una capa sobre cualquier
        fondo.
      - Seleccionado: fondo de acento con marca lateral, y un tono más apagado
        cuando el foco está en otra zona.
      - Anillo de foco de 2 px, por dentro en las filas.
      - Deshabilitado: atenuado y sin reacción; los botones rellenos pierden
        el relleno.

      *Tests:* contraste también con *hover* y pulsado. Ninguna regla de
      estado usa el atajo `background`, y quitar el `outline` exige otra señal
      visible. Al hacer clic en una fila, el historial recibe el foco.
      *Verificado manualmente:*
      - *hover* y pulsado en botones, filas, archivos, pestañas y menú;
      - selección con y sin foco (commit, archivo y stash);
      - anillo de foco con Tab;
      - estados deshabilitados (Commit sin resumen, y Push/Pull sin remoto).

      El usuario aprobó que el pulsado oscurezca y que el historial reciba el
      foco.
- [x] **A.3 Tooltips y menús de la barra.**
      - Todo tooltip se coloca justo antes de mostrarse para quedar dentro
        de la ventana: se desplaza en horizontal y se muestra encima si no
        cabe debajo.
      - Los desplegables junto a Abrir repositorio, Fetch, Pull y Push usan
        un chevron visible en un botón de 28 × 28 px, con el tooltip «Más
        opciones de …».

      *Tests:* colocación junto a cada borde y encima, y el tooltip y el
      icono de los tres menús remotos.
      icono de los tres menús remotos.
      *Verificado manualmente:* el tooltip de Operaciones queda dentro de la
      ventana y los chevrons se ven.

### B. Panel de cambios sin confirmar
- [x] **B.1 Cabecera y caja de commit fija.** La cabecera dice «N cambios en
      `<rama>`» (o «en HEAD separado»). La caja de commit queda fija abajo,
      separada de la lista por un divisor redimensionable con ratón y con
      teclado (`role="separator"`, flechas). Tiene límites de altura y su
      altura se recuerda.
      *Estado:* la lista de cambios tiene su propio scroll y la caja de
      commit queda debajo del divisor.
      - Con el teclado: ↑/↓ cambian la altura de 16 en 16 px, Inicio/Fin van
        a los límites, e Intro o un doble clic vuelven a la altura natural.
      - Límites: mínimo 140 px, y siempre quedan al menos 120 px para la
        lista.
      - La altura se guarda en el `localStorage` del webview, junto con las
        demás preferencias de vista del panel.

      *Tests:* textos de la cabecera (con rama y con HEAD separado), la caja
      de commit fuera de la lista, y el redimensionado con teclado y su
      persistencia.
      *Verificado manualmente:* la cabecera «N cambios en main» y «HEAD
      separado» (tras `git switch --detach` y vuelta con `git switch
      main`), y el divisor (ratón y teclado; se recuerda al reiniciar).
- [x] **B.2 Secciones plegables con acciones.** «Conflictos», «Sin preparar»
      y «Preparados» se pliegan y despliegan, cada una con su contador y sus
      acciones siempre visibles: *Preparar todo*, *Quitar todo* y *Descartar
      todo*. *Descartar todo* usa la confirmación y el «Deshacer» de la 4.4.
      El plegado se recuerda.
      *Estado:* la cabecera de cada sección es un botón con chevron
      (`aria-expanded`). Las acciones siguen visibles con la sección plegada,
      y se desactivan cuando no hay nada sobre lo que actuar. El plegado es
      el mismo en todos los repositorios.
      *Tests:* plegar y recordar al volver a montar, acciones de una sección
      vacía desactivadas, y «Descartar todo» pasa por la vista previa del
      descarte.
      *Verificado manualmente:* secciones plegables con los botones a la
      vista, y «Descartar todo» con confirmación.
- [x] **B.3 Vista de árbol.** Un conmutador cambia entre lista por ruta y
      árbol de carpetas. En el árbol, las carpetas se pliegan, muestran su
      número de archivos y hay *Expandir todo* / *Plegar todo*. Se navega con
      teclado (`role="tree"`: flechas para moverse, ← pliega y → despliega).
      La vista elegida se recuerda entre sesiones.
      *Estado:* conmutador Lista/Árbol en la cabecera del panel. En el árbol
      se muestran primero las carpetas, en orden natural (`a 2` va antes que
      `a 10`), y cada archivo aparece solo con su nombre. Un directorio sin
      seguimiento (`notes/`) es una hoja: git no lista su contenido.
      - Teclado: además de las flechas, Inicio y Fin, Intro abre el diff de
        un archivo o pliega una carpeta, y Espacio prepara o quita el
        archivo. Los botones de cada fila no entran en el orden de Tab (solo
        una fila a la vez), así que con el teclado el descarte por archivo
        sigue en la vista de lista.
      - El plegado de carpetas dura la sesión. Los conflictos se muestran
        siempre como lista.

      *Tests:* construcción del árbol (contadores, orden y rutas que empiezan
      por `-`), conmutar y recordar la vista, plegar con clic, Expandir y
      Plegar todo, y teclado.
      *Verificado manualmente:* vista de árbol con contadores, Expandir y
      Plegar todo, y navegación con teclado.
- [x] **B.4 Ordenar y carpetas enteras.** Se ordena por nombre o por estado
      en las dos vistas, y el orden se recuerda. Desde el árbol se prepara o
      se quita una carpeta entera (botón en la fila y menú contextual). La
      operación pasa la lista explícita de archivos de la carpeta, no un
      patrón.
      *Tests:* construcción del árbol y contadores, orden, preparar o quitar
      una carpeta con rutas que contienen espacios o empiezan por `-`.
      *Estado:* selector Nombre/Estado en la cabecera del panel.
      - Por estado: modificados, añadidos, eliminados, renombrados… y
        después sin seguimiento; dentro de cada grupo, por nombre. En el
        árbol, las carpetas siguen yendo primero.
      - Cada carpeta tiene el botón Preparar (o Quitar), la entrada en su
        menú contextual y la tecla Espacio.
      - Se envían los archivos que hay debajo de la carpeta: los renombrados
        preparados van con sus dos nombres, y una carpeta hermana con un
        nombre parecido (`-dir2`) no se toca. Test de integración en el
        núcleo con `-dir/a b.txt`.
        núcleo con `-dir/a b.txt`.

      *Verificado manualmente:*
      - preparar y quitar una carpeta entera: `git status` confirma que solo
        se preparó `src/ui/`, y al quitar `docs/` el renombrado pasa a D + ?,
        como hace git;
      - orden por estado.
- [x] **B.5 Ajustes tras la verificación.**
      - La barra del panel ya no recorta texto: Expandir/Plegar todo son
        botones de icono con tooltip, y si falta ancho la barra pasa a dos
        líneas.
      - En toda la app se dice «HEAD separado» (en inglés, «detached HEAD»).
      - En el árbol, Supr o ⌫ sobre un archivo sin preparar lo descarta, con
        la confirmación y el «Deshacer» de siempre. Se añadirá a la lista de
        atajos en D.5.

      *Tests:* botones de icono con su tooltip, reglas CSS de ajuste de
      línea (jsdom no calcula el layout) y Supr/⌫ solo en archivos sin
      preparar.

### C. Pestaña nueva
- [x] **C.1 Botón «+» y ⌘T.** El botón «+» de la barra de pestañas, ⌘T y
      *Archivo → Nueva pestaña* abren una pestaña con la pantalla de inicio
      (Abrir, Clonar, Inicializar y Recientes). Al abrir, clonar o
      inicializar un repo desde ahí, esa pestaña pasa a ser la del repo. Si
      el repo ya estaba abierto en otra pestaña, se activa esa y se cierra la
      vacía. Si ya hay una pestaña vacía, ⌘T va a ella en vez de crear otra.
      ⌘W cierra la pestaña vacía. Las pestañas vacías no se guardan en la
      sesión.
      *Estado:* la pestaña nueva es una sola y siempre la última.
      - ⌘1…⌘9 y ⌃Tab la cuentan, y ⌘T está en el menú nuevo *Archivo →
        Nueva pestaña*.
      - Al cerrarla, con × o ⌘W, se activa la pestaña de su izquierda.
      - Si se abre un repo desde otra pestaña, la pestaña nueva se queda
        donde está, sin activar.
      - Si se sale con ella activa, la sesión guarda como activa la pestaña
        de su izquierda.

      *Tests:* abrir con «+» y ⌘T (sin duplicar), convertirse al abrir,
      clonar o inicializar, ir a la pestaña existente si el repo ya está
      abierto, cerrar con × y ⌘W, y que no se guarde en la sesión.

### D. Ajustes (engranaje y ⌘,)
- [ ] **D.1 Ventana de Ajustes y Generales.** El engranaje de la barra y
      *GitCRAFT → Ajustes…* (⌘,) abren los Ajustes, con las secciones
      Generales, Identidad, Cuenta de GitHub y Atajos. En Generales:
      - idioma (es/en), que se aplica al momento;
      - tamaño de texto, en varios niveles, que escala toda la UI;
      - carpeta por defecto para clonar.

      Se guardan en `settings.json`.
- [ ] **D.2 Rama inicial, pull por defecto y fetch automático.** Rama
      inicial por defecto (validada con `git check-ref-format`) y estrategia
      de pull por defecto (merge, rebase o solo fast-forward). Se guardan en
      la **config global de git** (`init.defaultBranch`, `pull.rebase` /
      `pull.ff`, con `git config --global`), así que también afectan a git en
      la terminal, y Ajustes lo avisa. Ajustes muestra el valor actual, y el
      modo de pull elegido por repo en GitCRAFT (6.2) sigue teniendo
      prioridad. Fetch automático cada N minutos,
      **desactivado por defecto**:
      - se hace en las pestañas abiertas que tienen remoto;
      - no se lanza si ya hay una operación remota en ese repo;
      - sus fallos se ven en el estado remoto, sin ventanas emergentes;
      - cada fetch aparece en el panel de operaciones.
- [ ] **D.3 Identidad de Git.** `user.name` y `user.email` en dos ámbitos:
      el global y el del repositorio de la pestaña activa, con el valor
      efectivo y de dónde viene. Se pueden editar los dos y quitar el del
      repo para heredar el global. Se lee y escribe con `git config`
      (`--global` / `--local`, salida con `-z`). Validación básica del email.
      *Tests de integración* con `GIT_CONFIG_GLOBAL` temporal.
- [ ] **D.4 Cuenta de GitHub con `gh`.** Detecta si `gh` está instalado; si
      no, explica cómo instalarlo (`brew install gh` o cli.github.com). Lee
      el estado con `gh auth status --hostname github.com --json hosts` y
      muestra con qué usuario hay sesión o si el token no es válido.
      - **Iniciar sesión dentro de la app:** se lanza `gh auth login
        --hostname github.com --web --git-protocol https`. GitCRAFT muestra el
        código de un solo uso con un botón de copiar, abre el navegador y
        deja cancelar.
      - **Cerrar sesión:** `gh auth logout`, con confirmación.
      - **Botón «Usar gh para las credenciales de Git»:** ejecuta `gh auth
        setup-git`. Antes explica qué cambia en `~/.gitconfig`: añade gh
        como credential helper de github.com, que hoy es `osxkeychain`.
        Nunca se ejecuta automáticamente.

      GitCRAFT **no lee ni guarda ningún token**: nunca usa `gh auth token`
      ni `--show-token`.
      *Tests sin red* con un `gh` simulado (script de prueba en el PATH):
      sin instalar, sin sesión, con sesión, token caducado y cancelar el
      inicio de sesión.
- [ ] **D.5 Atajos (solo consulta).** Lista de todos los atajos de la app,
      agrupados (incluidos los del árbol de cambios, como Supr/⌫ para
      descartar). Sale del mismo registro que usan los manejadores de teclado
      y el menú.
      *Test:* falla si un atajo registrado no aparece en la lista o al
      revés. Personalizarlos queda para la fase 3.

### E. Publicar un repo local en GitHub
- [ ] **E.1 Núcleo de publicación.** El repo siempre se crea en la **cuenta
      personal**; las organizaciones quedan para más adelante. En este orden:
      1. Comprueba que el nombre no existe en la cuenta (`gh repo view
         <usuario>/<nombre> --json name`). Si existe, lo explica y no toca
         nada.
      2. Crea el repo con `gh repo create <nombre> --private|--public
         [--description …]`. **Privado por defecto.**
      3. Añade `origin`.
      4. Hace push de la rama actual con `--set-upstream`, con la misma
         maquinaria de progreso y cancelación de la 6.x. **Nunca push
         forzado.**

      Si el push falla después de crear el repo, se explica qué ha quedado
      hecho (repo remoto creado y `origin` añadido) y se puede reintentar con
      Push. El repo remoto nunca se borra automáticamente.
      *Tests sin red* con `gh` simulado y un remoto `file://` como destino:
      creación, nombre existente (no se ejecuta ningún paso más), sin sesión,
      fallo de push tras crear y argumentos (el nombre y la descripción van
      como argumentos separados, nunca interpolados).
- [ ] **E.2 Diálogo «Publicar en GitHub».** Aparece en un repo sin remotos.
      Tiene:
      - nombre propuesto a partir de la carpeta, ajustado a los caracteres
        válidos en GitHub;
      - descripción opcional;
      - visibilidad, con **Privado** marcado por defecto;
      - progreso por pasos.

      Antes de confirmar se muestra un resumen de lo que se va a subir: rama,
      número de commits y tamaño aproximado. Se avisa, **sin bloquear**, en
      estos casos:
      - el repo no tiene `.gitignore`;
      - hay archivos de más de 50 MB en lo que se sube;
      - hay carpetas típicas de compilación commiteadas (`build/`, `bin/`,
        `target/`, `node_modules/`, `dist/`).

      Si no hay sesión de GitHub, lleva a Ajustes → Cuenta de GitHub. Si no
      hay commits o HEAD está separado, el botón está desactivado y explica
      por qué.
      *La prueba real la hace el usuario* con un repo de prueba privado que
      luego borra.

### Cierre de la fase
- [ ] **V.1 Versión 0.2.0.** Versión 0.2.0 en `package.json`, `Cargo.toml` y
      `tauri.conf.json`; README actualizado; `.dmg` generado. El tag anotado
      `v0.2.0` se crea solo cuando el usuario confirme la verificación.

## Fase 2

- [ ] **8. Merge/rebase con drag & drop.** Al arrastrar una rama sobre otra se
      abre un diálogo de confirmación que muestra qué rama se mueve y cuántos
      commits.
- [ ] **9. Conflictos.** Vista a tres paneles (ours / theirs / resultado),
      elección por bloque, marcar como resuelto, continuar o abortar.
      Incluye continuar un merge/rebase tras resolver conflictos desde la app
      (`git merge --continue` / `git rebase --continue` / `--skip`); hoy (6.2)
      solo se puede abortar desde la app y continuar se hace en la terminal.
- [ ] **10. Cherry-pick, revert, reset** (soft/mixed/hard con aviso) y **crear
      tags** (ligeros y anotados).
- [ ] **11. Deshacer/rehacer** de la última operación cuando sea seguro, basado
      en el reflog. Antes de ejecutarlo se muestra qué se va a restaurar.
- [ ] **12. Búsqueda** por mensaje, autor, hash y archivo; **historial y blame**
      de un archivo.
- [ ] **13. Rebase interactivo visual:** reordenar, squash, editar mensaje y
      descartar.

## Fase 3

- [ ] Submódulos · Worktrees · Git LFS · Firma GPG/SSH (verificar y mostrar) ·
      Perfiles de identidad · Atajos configurables · Tema claro/oscuro ·
      Builds de Windows y Linux · Firma y notarización en macOS.
- [ ] Inicio de sesión de GitHub propio dentro de la app (OAuth *device
      flow*, token guardado en el Llavero de macOS), como alternativa a `gh`.
