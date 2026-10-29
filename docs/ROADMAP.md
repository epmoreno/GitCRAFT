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

### 2. Grafo de commits
- [x] **2.1 Parser de log.** `git log --topo-order -z --format=<campos separados
      por %x00/%x1f>` se parsea a `Commit { oid, parents, author, … }`. *Tests
      unitarios:* mensajes con saltos de línea, UTF-8, commits raíz, merges con
      más de 2 padres y firmas vacías.
- [x] **2.2 Refs.** `git for-each-ref` produce ramas, remotos, tags y HEAD (también
      HEAD desacoplado). *Tests unitarios + integración.*
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
      confirmación). Sin upstream, upstream borrado y HEAD desacoplado se
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
- [ ] **T.1 Errores.** Un catálogo traduce los errores conocidos de git a
      mensajes claros, con la salida original desplegable.
- [ ] **T.2 Panel de operaciones.** Muestra las operaciones en curso con progreso
      y botón de cancelar, y un historial de comandos ejecutados.
- [x] **T.3 i18n.** Todos los textos de la UI están en `es` y `en`; hay un test
      que falla si falta una clave.
      *Estado:* `src/i18n/i18n.test.ts` (desde la 0.3) falla si una clave
      está en un idioma y no en el otro, o si un texto está vacío. Se añade,
      sin duplicarlo, lo que no cubría: cualquier test falla si un componente
      usa una clave que no existe (`missingKeyHandler` en
      `src/test/setup.ts`), así que una clave mal escrita no llega a mostrarse
      como texto crudo.
- [ ] **T.4 Empaquetado.** `.dmg` para Apple Silicon (sin firmar en el MVP).

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
