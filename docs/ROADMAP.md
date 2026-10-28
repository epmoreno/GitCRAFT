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

### 1. Repositorios
- [x] **1.1 Abrir repo.** Diálogo nativo → validación con
      `git rev-parse --show-toplevel` → aparece la pestaña. Abrir una carpeta que
      no es un repo da un error claro. *Test:* integración con repo y no-repo.
- [x] **1.2 Recientes.** La lista persiste entre sesiones (máx. 20) y quita
      entradas de rutas que ya no existen.
- [ ] **1.3 Pestañas.** Varias pestañas con estado independiente; se pueden
      cerrar y reordenar; se restauran al reiniciar.
- [ ] **1.4 Inicializar repo.** `git init` con rama inicial configurable.
- [ ] **1.5 Clonar.** URL + destino, progreso (`--progress`) y cancelación; la
      autenticación la gestiona el credential helper. *Test:* clonar un repo
      local `file://`.

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
- [x] **2.5 Render virtualizado.** Canvas para el grafo y filas virtualizadas
      para el texto; scroll a 60 fps con 50k commits. Muestra etiquetas de
      ramas, tags y HEAD.
      *Estado:* scroll fluido con git/git (~78k commits), comprobado en la app.
- [x] **2.6 Refresco.** Un watcher de `.git` (con debounce) refresca refs y el
      grafo tras cambios externos (por ejemplo, un commit hecho en la terminal).
      *Estado:* vigila HEAD, `packed-refs` y `refs/` del git dir (y del común en
      worktrees) con FSEvents; debounce de 300 ms; las operaciones propias se
      silencian. La UI mantiene las filas y la selección hasta tener el grafo nuevo.

### 3. Detalle de commit
- [x] **3.1 Metadatos.** Autor, committer, fechas, mensaje completo, padres
      navegables y refs.
- [x] **3.2 Archivos cambiados.** `git diff-tree -r -z --name-status -M` con
      estado A/M/D/R/C y contadores +/−. *Tests:* renombrados, binarios y commit
      raíz.
- [ ] **3.3 Diff.** Parser de diff unificado (hunks, líneas, no-newline-at-EOF,
      binarios) y vista con números de línea y resaltado. Los archivos grandes
      se cargan bajo demanda.

### 4. Working directory
- [ ] **4.1 Estado.** `git status --porcelain=v2 -z --branch` se parsea a
      unstaged, staged, untracked y conflictos, además de ahead/behind. *Tests
      unitarios.*
- [ ] **4.2 Stage/unstage por archivo** (también todos a la vez).
- [ ] **4.3 Stage/unstage por hunk y por línea.** Se genera un parche y se aplica
      con `git apply --cached` (con `-R` para unstage). *Tests:* hunks
      parciales, líneas sueltas y finales de archivo.
- [ ] **4.4 Descartar cambios** con confirmación.
- [ ] **4.5 Commit.** Resumen + descripción, contador de caracteres; amend
      precarga el último mensaje. Respeta hooks (`pre-commit`, `commit-msg`) y
      muestra su salida si fallan.

### 5. Ramas
- [~] **5.1 Barra lateral.** Ramas locales, remotas (agrupadas por remoto), tags y
      stashes, con filtro.
      *Estado:* ramas locales, remotos y tags hechos (clic → commit en el
      grafo); faltan stashes (7.x) y el filtro.
- [ ] **5.2 Crear, checkout, renombrar y borrar.** Borrar una rama no fusionada
      pide confirmación y explica qué commits quedan inaccesibles. Para hacer
      checkout de una rama remota se crea una rama de seguimiento.

### 6. Remotos
- [ ] **6.1 Fetch** (todos los remotos o uno, `--prune` opcional) con progreso y
      cancelación.
- [ ] **6.2 Pull** con modo merge o rebase configurable por repo (por defecto,
      lo que diga `pull.rebase`).
- [ ] **6.3 Push** y push con `--set-upstream`. Si el push es rechazado (non
      fast-forward) se explica y se ofrece pull. El force push usa
      `--force-with-lease` y pide confirmación.
- [ ] **6.4 Indicadores ahead/behind** en la barra lateral y en la toolbar.

### 7. Stash
- [ ] **7.1 Guardar** (con mensaje, opción de incluir untracked), **aplicar**,
      **pop** y **borrar** (con confirmación).
- [ ] **7.2 Ver contenido** de un stash con la vista de diff.

### Transversal del MVP
- [ ] **T.1 Errores.** Un catálogo traduce los errores conocidos de git a
      mensajes claros, con la salida original desplegable.
- [ ] **T.2 Panel de operaciones.** Muestra las operaciones en curso con progreso
      y botón de cancelar, y un historial de comandos ejecutados.
- [ ] **T.3 i18n.** Todos los textos de la UI están en `es` y `en`; hay un test
      que falla si falta una clave.
- [ ] **T.4 Empaquetado.** `.dmg` para Apple Silicon (sin firmar en el MVP).

## Fase 2

- [ ] **8. Merge/rebase con drag & drop.** Al arrastrar una rama sobre otra se
      abre un diálogo de confirmación que muestra qué rama se mueve y cuántos
      commits.
- [ ] **9. Conflictos.** Vista a tres paneles (ours / theirs / resultado),
      elección por bloque, marcar como resuelto, continuar o abortar.
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
