# Benchmarks

## Grafo de commits (ROADMAP 2.4)

Mide desde que se lanza `git log` hasta la última fila con su layout, sin la
UI. Comando: `npm run bench:graph` (release; mediana de 3 ejecuciones).
Opciones: `GITCRAFT_BENCH_COMMITS=<n>` y `GITCRAFT_BENCH_REPO=<ruta>` para medir
además un repo real.

Objetivos: primera página < 150 ms y grafo completo de 100k commits < 3 s.

### 2026-10-27 — Apple M4, git 2.55.0, 100.000 commits sintéticos

Historial sintético: `main` con hasta 4 ramas de feature paralelas que se
fusionan (1 de cada 3 se abandona sin fusionar) y un tag cada 1000 commits.
Generarlo con `git fast-import` tarda 1,8 s.

| Escenario | Primera página (256 filas) | Grafo completo | JSON (IPC) |
| --- | --- | --- | --- |
| Sin `commit-graph` | 294 ms ❌ | 428 ms ✅ | 42,9 MB en 27 ms |
| Con `commit-graph` | 81 ms ✅ | 535 ms ✅ | 42,9 MB en 28 ms |

Conclusiones:

- **Total:** unas 6 veces por debajo del objetivo. El layout y el parseo
  cuestan poco; casi todo el tiempo es de `git log`.
- **Primera página sin `commit-graph`:** no cumple. `--topo-order` necesita
  recorrer todo el historial antes de emitir el primer commit. Con
  `commit-graph` (git lo escribe en `gc`/`maintenance` y muchos repos ya lo
  tienen) emite en streaming.
  Decisión (2026-10-28): GitCRAFT nunca lo genera sin permiso. En repos de
  más de 10.000 commits sin `commit-graph` muestra un aviso discreto con el
  botón «Optimizar repositorio», que ejecuta
  `git commit-graph write --reachable --changed-paths` con progreso y
  cancelación. El aviso se puede descartar para cada repo.
- **Tamaño del JSON:** 43 MB para 100k commits (~430 bytes por commit) pasan por
  el puente IPC y el WebView tiene que parsearlos. Reducido el 2026-10-28
  (ver abajo).
- La anchura del sintético es pequeña (máx. 4 columnas). Para medir un repo
  con muchas ramas paralelas, usa `GITCRAFT_BENCH_REPO`.

### 2026-10-28 — Solo los datos de la fila (decisión: el detalle se pide aparte)

Cada fila envía ahora el oid, el asunto, el autor, la fecha, si es merge, y los
carriles como pares `[columna, color]`. Ya no se envían los padres, el email
ni la fecha del committer: el detalle del commit se carga al seleccionarlo.

| Escenario | Primera página | Grafo completo | JSON (IPC) |
| --- | --- | --- | --- |
| Sin `commit-graph` | 291 ms | 429 ms | **22,6 MB** en 14 ms (antes 42,9 MB en 27 ms) |
| Con `commit-graph` | 83 ms | 533 ms | **22,6 MB** en 14 ms |

Es un 47 % menos de datos para el WebView (~226 bytes por commit). Los tiempos
de carga no cambian porque los domina `git log`. En repos anchos el ahorro es
mayor, porque cada fila repite sus carriles que pasan.

## Búsqueda, historial de un archivo y blame (ROADMAP 12.7)

Mide desde que se lanza git hasta el primer lote y hasta el final, sin la
UI. Comando: `npm run bench:search` (release; mediana de 3 ejecuciones).
Opciones: `GITCRAFT_BENCH_COMMITS=<n>` (50.000 por defecto) y, para un repo
real, `GITCRAFT_BENCH_REPO=<ruta>` con `GITCRAFT_BENCH_FILE=<archivo>`.

Objetivos (tarea 12): búsqueda en mensajes, primer resultado < 300 ms y
completa < 3 s; historial de un archivo, primera página < 500 ms con
`commit-graph`; blame, primeras líneas < 1 s.

### 2026-11-16 — Apple M4, git 2.55.0

**Sintético, 50.000 commits** (cada uno cambia uno de 200 archivos; uno de
cada diez añade una línea a `src/grande.txt`, que acaba con 5.000 líneas y
5.000 commits). Generarlo con `git fast-import` tarda 5,3 s.

| Medida | Sin `commit-graph` (primero / todo) | Con `commit-graph --changed-paths` |
| --- | --- | --- |
| Mensaje «change 4», literal (11.111) | 6 / 240 ms ✅ | 6 / 231 ms ✅ |
| Mensaje que no aparece, literal (todo el historial) | — / 236 ms ✅ | — / 229 ms ✅ |
| Mensaje «^change 4», expresión regular | 5 / 187 ms ✅ | 5 / 180 ms ✅ |
| Autor «a», literal (50.000) | 5 / 246 ms | 5 / 233 ms |
| Archivo «src/grande.txt» (5.000) | 31 / 1.269 ms | 28 / 1.140 ms |
| Historial de `src/grande.txt` (5.000) | 25 / 741 ms | 22 / 644 ms ✅ |
| Blame de `src/grande.txt` (5.000 líneas) | 65 / 1.179 ms ✅ | 52 / 554 ms ✅ |

**git/git** (clon completo, 83.007 commits, `eacee88cf8`; `diff.c`: 7.846
líneas y 1.201 commits en su historial). Medido tal como se clona y tras
`git commit-graph write --reachable --changed-paths` (4,1 s).

| Medida | Sin `commit-graph` (primero / todo) | Con `commit-graph --changed-paths` |
| --- | --- | --- |
| Mensaje «fix», literal (21.144) | 19 / 623 ms ✅ | 21 / 615 ms ✅ |
| Mensaje que no aparece, literal (todo el historial) | — / 610 ms ✅ | — / 613 ms ✅ |
| Mensaje «^fix», expresión regular (3.076) | 36 / 647 ms ✅ | 37 / 629 ms ✅ |
| Autor «a», literal (77.886) | 17 / 582 ms | 17 / 582 ms |
| Archivo «diff.c» (20.196: toda ruta que lo contiene) | 50 / 9.018 ms | 50 / 8.381 ms |
| Historial de `diff.c` (1.201) | 57 / 975 ms ✅ | 58 / 626 ms ✅ |
| Blame de `diff.c` (7.846 líneas) | 67 / 612 ms ✅ | 58 / 346 ms ✅ |

Conclusiones:

- **Todos los objetivos se cumplen**, también sin `commit-graph`: el primer
  resultado llega en decenas de milisegundos y la búsqueda completa de
  git/git en ~0,6 s.
- **El `commit-graph` con `--changed-paths`** ayuda donde git mira rutas:
  el historial de un archivo (−36 % en git/git) y el blame (−43 %). A los
  mensajes casi no les afecta.
- **La búsqueda por archivo es la más lenta** (8–9 s en git/git): el
  pathspec «contiene el texto» (`*diff.c*`, que también encuentra
  `builtin/diff.c`, `xdiff/xdiff.c`…) obliga a git a comparar los árboles de
  cada commit y no puede usar los filtros de Bloom. Llega por partes desde
  los 50 ms y se puede cancelar, así que no bloquea nada. Mejora posible:
  cuando el texto es una ruta exacta que existe, buscar solo esa ruta
  (git sí usa entonces los filtros de Bloom). Hecha el 2026-11-17 (abajo).

### 2026-11-17 — Búsqueda por la ruta exacta (Apple M4, git 2.55.0)

Si el texto es un archivo o una carpeta del índice, la búsqueda por
archivo busca solo esa ruta (`:(literal)diff.c`) en vez de toda ruta que
contenga el texto (`*diff.c*`). Mismo banco y mismo clon de git/git; «sin
`commit-graph`» se mide desactivándolo con `core.commitGraph=false`.

| Archivo «diff.c» en git/git | Sin `commit-graph` (primero / todo) | Con `commit-graph --changed-paths` |
| --- | --- | --- |
| Antes: toda ruta que lo contiene (20.196) | 50 / 9.018 ms | 50 / 8.381 ms |
| Ahora: la ruta exacta (18.200) | 26 / 1.376 ms | 24 / 427 ms |

En el sintético de 50.000 commits («src/grande.txt», 5.000 resultados)
apenas cambia: 35 / 1.268 ms y 32 / 1.153 ms. Allí cada commit toca un
solo archivo y comparar los árboles ya era barato.

Los 18.200 resultados de «diff.c» son casi todos merges: con
`--full-history`, git lista cada merge en el que el archivo difiere de
alguno de sus padres (aunque el merge no lo cambie). Sin merges son 1.201,
los mismos que su historial.
