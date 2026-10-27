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
  Pendiente de decidir: ofrecer al usuario ejecutar
  `git commit-graph write --reachable` en repos grandes que no lo tengan.
- **Tamaño del JSON:** 43 MB para 100k commits (~430 bytes por commit) pasan por
  el puente IPC y el WebView tiene que parsearlos. Si el parseo en WebKit
  resulta lento, el siguiente paso es un formato compacto (arrays en lugar de
  objetos con claves, o binario).
- La anchura del sintético es pequeña (máx. 4 columnas). Para medir un repo
  con muchas ramas paralelas, usa `GITCRAFT_BENCH_REPO`.
