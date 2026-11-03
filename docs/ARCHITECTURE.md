# GitCRAFT — Arquitectura

> Estado: **APROBADA** (2026-10-27).

## 1. Stack y justificación

| Capa | Elección | Por qué |
| --- | --- | --- |
| Shell de escritorio | **Tauri 2** | Usa el WebView del sistema (WKWebView en macOS): un binario de ~10–15 MB frente a ~150 MB con Electron, y bastante menos RAM y arranque más rápido. Backend en Rust, ideal para lanzar procesos, parsear y calcular el layout del grafo. Permisos (capabilities) explícitos por comando. |
| Núcleo Git | **Rust** (`tokio::process`) + binario `git` del sistema | Máxima compatibilidad con la config, los hooks, las credenciales y la firma del usuario. |
| Grafo | `git log` en streaming + layout en Rust | Detrás del trait `CommitSource`, para poder cambiar a `git2` si los benchmarks lo justifican (decisión ROADMAP 2.4). |
| UI | **React 19 + TypeScript + Vite** | Ecosistema maduro y es tu preferencia. |
| Estado | **Zustand** (estado de la UI y de las pestañas) + **TanStack Query** (caché de datos de Git con invalidación) | Ligeros. Query resuelve la caché, el refetch y la cancelación. |
| Listas grandes | **@tanstack/react-virtual** + `<canvas>` para el grafo | El canvas dibuja solo las filas visibles; el DOM solo tiene ~60 filas. |
| Primitivas UI | **Radix UI** (sin estilos) + CSS Modules con tokens propios | Accesibilidad (menús, diálogos, foco) sin imponer un aspecto: la identidad visual es 100 % nuestra. |
| i18n | **i18next** (`es`, `en`) | Estándar. Un test detecta claves que faltan. |
| Tipos IPC | **tauri-specta** (`=2.0.0-rc.26`, fijado) | Los tipos TS de comandos y eventos se generan desde Rust en `src/ipc/bindings.ts` (`npm run bindings`). Un test de Rust falla si el archivo está desactualizado. Los comandos devuelven `{status: "ok", data}` o `{status: "error", error: IpcError}`. |
| Tests | `cargo test` + `tempfile` + `insta` (snapshots de parsers) · Vitest + Testing Library | |

**¿Y Electron?** Solo lo recomendaría si necesitáramos Chromium idéntico en todas
las plataformas o mucho código Node. El único punto débil real de Tauri es
WebKitGTK en Linux (rendimiento irregular), y lo mitigamos dibujando el grafo en
canvas y virtualizando. Para macOS primero, Tauri es claramente mejor.

## 2. Estructura del repositorio

```text
GitCRAFT/
├─ Cargo.toml                 # workspace
├─ crates/
│  └─ gitcraft-core/          # Rust puro, SIN Tauri → se testea aislado
│     └─ src/
│        ├─ runner/           # GitRunner: spawn, env, streaming, cancelación, timeout
│        ├─ parse/            # log, refs, status(v2), diff, diff_tree, stash… (funciones puras)
│        ├─ graph/            # layout de carriles incremental
│        ├─ ops/              # operaciones de alto nivel (open, commit, branch, stage_lines…)
│        ├─ error.rs          # GitError + clasificador de stderr → ErrorKind
│        └─ testing.rs        # TestRepo (feature "testing"): repos temporales deterministas
├─ src-tauri/                 # app Tauri: capa fina
│  └─ src/
│     ├─ commands/            # #[tauri::command] → delegan en gitcraft-core
│     ├─ session.rs           # RepoSession por pestaña (cola de escritura, caché del grafo)
│     ├─ jobs.rs              # JobManager: id, progreso, cancelación
│     ├─ watcher.rs           # notify + debounce sobre .git y el worktree
│     └─ settings.rs          # recientes, pestañas, preferencias (JSON en app_config_dir)
├─ src/                       # frontend React
│  ├─ ipc/                    # bindings.ts (generado) + wrappers
│  ├─ features/{repo,graph,commit,workdir,branches,remotes,stash,jobs}/
│  ├─ ui/                     # componentes base y tokens de diseño
│  ├─ i18n/{es,en}.json
│  └─ app/                    # layout de 3 zonas, pestañas, toolbar
└─ docs/
```

### Copia remota

El repositorio tiene una copia de seguridad en GitHub, en un repositorio
**privado**: `origin` → `https://github.com/epmoreno/GitCRAFT.git`. `main`
sigue a `origin/main`, y las versiones se publican como tags anotados
(`v0.1.0`, el MVP). Nunca se hace push forzado.

Las credenciales de GitHub se gestionan con **GitHub CLI** (`gh auth login`).
Git las toma del credential helper configurado en el sistema (`osxkeychain`,
en `/opt/homebrew/etc/gitconfig`); `gh auth setup-git` haría que Git las
pidiera directamente a `gh`. No se guardan tokens en el repositorio ni en su
configuración.

## 3. Flujo de un comando Git (ida y vuelta)

```text
 UI (React)                     Tauri (src-tauri)                  gitcraft-core                    SO
 ─────────                      ─────────────────                  ─────────────                    ──
 click "Fetch"
   │ commands.fetch(repoId, opts, channel)   ← tipado por tauri-specta
   ├──────────────────────────► fetch()
   │                             │ session = sessions[repoId]
   │                             │ job = jobs.start(kind=Fetch)  → JobId + CancellationToken
   │                             │ session.write_queue.run(…)    (escrituras serializadas por repo)
   │                             ├────────────────────────────► ops::fetch(&runner, opts, token, on_progress)
   │                             │                                │ GitRunner::new(repo)
   │                             │                                │   .args(["fetch","--progress","--prune","--", remote])
   │                             │                                │   .spawn()  ─────────────────────► git (proceso propio,
   │                             │                                │                                      grupo de procesos propio)
   │◄── Channel<Progress> ───────┤◄── on_progress(Progress) ──────┤◄── stderr "Receiving objects: 42%" ┤
   │   (barra de progreso)       │                                │
   │ [Cancelar] → jobs.cancel(id)│── token.cancel() ─────────────►│ killpg(SIGTERM) → 2 s → SIGKILL ──►┤
   │                             │                                │◄── exit status + stdout/stderr ────┘
   │                             │                                │ parse / classify error
   │◄── Result<FetchResult, ─────┤◄── Result<_, GitError> ────────┘
   │       IpcError>             │ emit("repo-changed", {repoId, scopes:[Refs]})
   │ queryClient.invalidate([repoId,"refs"]) → refetch del grafo/sidebar
```

### Puntos clave

- **GitRunner** (core):
  - `tokio::process::Command::new(git_path)` con `args: Vec<OsString>`. **Nunca shell.**
  - Entorno de **todas** las invocaciones:
    - `LC_ALL=C`: la salida y los errores se pueden parsear sin depender del
      idioma del sistema.
    - `GIT_TERMINAL_PROMPT=0`: git nunca se queda esperando credenciales en una
      terminal invisible. La autenticación va por el credential helper.
  - Entorno de las **lecturas** (`status`, `log`, `diff`…): además
    `GIT_OPTIONAL_LOCKS=0`, para que no compitan por `index.lock` con las
    escrituras (ni con el git que el usuario use en la terminal).
  - Las operaciones que abrirían un editor llevan `GIT_EDITOR` y
    `GIT_SEQUENCE_EDITOR` controlados.
  - Opciones fijas: `-c core.quotepath=false` (rutas con caracteres no ASCII
    sin escapar) y `-c color.ui=false`.
  - Se lanza en su propio **grupo de procesos** (`process_group(0)`), para que al
    cancelar mueran también los hooks y `ssh` hijos. Además `kill_on_drop(true)`.
    *Windows (fase futura):* los grupos de procesos se sustituyen por **Job
    Objects**. No se implementa ahora.
  - Lectura en streaming de stdout (por registros `\0`) y de stderr (por `\r`/`\n`
    para el progreso).
  - Cada ejecución queda registrada (args, duración, exit code) en el historial
    del panel de operaciones.
- **Concurrencia por repo:** las lecturas (log, status, diff) van en paralelo y
  las escrituras (commit, checkout, fetch…) pasan por una cola serial por
  `RepoSession` (un `tokio::sync::Mutex` por repo, `AppState::write`) para
  evitar choques con `index.lock`; mientras duran, el vigilante se silencia.
- **Vigilante de `.git` y del worktree** (`src-tauri/src/watcher.rs`, crate
  `notify`). Clasifica cada cambio como `refs` (HEAD, `packed-refs`,
  `refs/**` del git dir y del common dir → se recarga el grafo) o `worktree`
  (archivos, `index`, `MERGE_HEAD`… → se recarga el estado). Agrupa eventos con
  un debounce de
  ~300 ms. Mientras haya una operación propia en curso en ese repo, sus eventos
  se **ignoran**; al terminar la operación se emite un único `repo-changed`.
  Así se evitan los refrescos en bucle (por ejemplo, un `status` que toca el
  índice y vuelve a disparar el watcher).
- **Errores:** `GitError::Failed { args, exit_code, stderr, stdout_tail }` y
  variantes propias. Un clasificador con patrones (con `LC_ALL=C` los mensajes
  son estables) los asigna a un `ErrorKind`; el catálogo completo está en
  «Errores hacia la UI» (sección 6).
- **Credenciales:** con `GIT_TERMINAL_PROMPT=0`, git usa el credential helper
  configurado (osxkeychain en macOS) o SSH. Si no hay credenciales disponibles,
  falla con `AuthFailed` y se explica cómo configurarlas. *(Más adelante, en
  Fase 3, se puede evaluar un `GIT_ASKPASS` propio que solo muestre un diálogo
  y no guarde nada.)*

## 4. Grafo de commits

1. **Lectura:** `git log --topo-order --format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%s%x00 -z --branches --remotes --tags HEAD`
   en streaming. El cuerpo del mensaje se pide aparte (en el detalle), así el log
   es ligero.
2. **Layout incremental (Rust):** se mantiene un vector de "carriles activos"
   (cada uno espera un oid). Por cada commit:
   - Se busca su carril (o se crea uno nuevo si es una punta de rama).
   - Se colapsan los carriles que esperaban el mismo oid (convergencias).
   - El primer padre hereda el carril; los padres extra reutilizan un carril
     libre o abren uno nuevo.
   - Se emite `Row { col, color, edges: [(from_col, to_col, kind)] }`.

   El estado es pequeño y continuable, así que cada página de N commits
   extiende el layout sin recalcular lo anterior. El color se asigna por carril
   con una paleta propia que rota (`--lane-1` … `--lane-8` en los tokens).
3. **Transferencia:** páginas de ~2 000 filas por `Channel`, en un formato
   compacto (índices en lugar de oids repetidos). Si se queda corto, se pasa a
   un buffer binario.
4. **Render:** el canvas dibuja solo el rango visible (las aristas que cruzan el
   viewport salen de los `edges` de las filas visibles); el texto de cada fila se
   renderiza en DOM virtualizado. Al acercarse al final se pide la siguiente
   página.
5. **Refs:** `git for-each-ref --format=… -z` produce un mapa oid → [etiquetas],
   que se pinta encima de las filas.

## 5. Stage por hunk y por línea

El frontend envía `(path, hunk_index, selected_line_indices)`. El core regenera
un parche mínimo a partir del diff ya parseado (las líneas `-` no seleccionadas
pasan a contexto, las `+` no seleccionadas se omiten y se recalculan las
cabeceras `@@`) y lo aplica con
`git apply --cached --whitespace=nowarn -` por stdin (con `-R`
para unstage). Esta lógica es pura y se cubre con tests unitarios exhaustivos.

## 5a. Panel de cambios sin confirmar

`src/features/workdir/WorkingDirPanel.tsx` ocupa todo el panel derecho.
Tiene tres partes:
- la cabecera («N cambios en `<rama>`»);
- la lista de cambios, con su propio scroll, que se recuerda por pestaña;
- la caja de commit fija abajo, con un divisor redimensionable
  (`ResizeHandle`, `role="separator"`) entre las dos.

Las preferencias de vista del panel (altura de la caja de commit y, desde
B.2–B.4, el plegado de secciones, la vista lista o árbol y el orden) están
en `viewPrefs.ts`. Se guardan con `persist` de zustand en el `localStorage`
del webview (`~/Library/WebKit/dev.gitcraft.app`). Son prescindibles: si se
pierden, el panel vuelve a su aspecto por defecto. Por eso no van en
`settings.json`, que guarda lo que sí importa (recientes, sesión, modos de
pull).

La vista de árbol (`fileTree.ts` y `ChangeList.tsx`) agrupa en el cliente
las rutas que devuelve `git status`. Se pinta como una lista plana de
`treeitem` con `aria-level`, así que cada fila tiene su propio *hover*. El
foco sigue el modelo de árbol de WAI-ARIA: solo una fila está en el orden de
Tab a la vez (*roving tabindex*). La vista de lista es el mismo árbol, plano
(todas las filas de nivel 1). Un clic enfoca siempre la fila entera, no el
botón pulsado, para que los atajos (Espacio, Intro, ⌘⌫) funcionen.

## 5b. Descartar cambios (la única operación que destruye trabajo)

`gitcraft-core/src/ops/discard.rs`. Solo afecta a cambios **sin preparar**; el
índice nunca se toca.

1. **Confirmación siempre**, con lo que se pierde archivo por archivo
   (`discard_preview`: +/− líneas, líneas de los archivos sin seguimiento,
   archivos de los directorios).
2. **Copia antes de tocar nada:**
   - Archivos con seguimiento: blobs exactos (`hash-object -w --no-filters`,
     más el modo y los enlaces simbólicos) para un «Deshacer» byte a byte, y una
     entrada de stash «GitCRAFT: descarte de …» (`stash create` + `stash
     store`) que sigue en `git stash list` aunque se cierre la app. Si todavía
     no hay commits, git no puede crear el stash y quedan solo los blobs.
   - Archivos sin seguimiento: se mueven a la Papelera (`NSFileManager
     trashItemAtURL`, que devuelve dónde quedan); nunca se borran.
3. **Descarte:** `git restore --worktree` para archivos completos y
   `git apply -R` con el mismo generador de parches que el stage parcial para
   hunks y líneas (con la misma comprobación de huella del diff).
4. **Deshacer** (aviso de 10 s): restaura los blobs y devuelve los archivos de
   la Papelera, pero solo si ninguno cambió desde el descarte; si no, no toca
   nada (`UndoConflict`). Tras deshacer se elimina la entrada de stash, que ya
   es redundante.
5. **Recuperación tardía** (pasado el aviso): siempre **por archivo**, con
   `git restore --source=<stash> --worktree -- <ruta>`, que devuelve el
   contenido que tenía el archivo justo antes del descarte y no toca el índice.
   **Nunca** `git stash apply`: aborta si el archivo tiene otros cambios
   (por ejemplo, partes preparadas), que es lo normal tras descartar un hunk o
   unas líneas. La app lo hace con «Recuperar» en las entradas
   «GitCRAFT: descarte de …» (`preview_recover` + `recover_file` en
   `ops/stash.rs`): `git diff <stash> -- <ruta>` muestra qué líneas del
   archivo actual no están en la copia (trabajo posterior, que se perdería);
   si no hay ninguna se recupera directamente y, si las hay, se piden
   confirmación listándolas. La recuperación lleva el hash del contenido
   comprobado (`hash-object --no-filters`) y se niega (`StaleDiff`) si el
   archivo cambió entretanto. *Tests:*
   `a_partial_discard_is_recoverable_from_the_stash_later` y
   `recovers_a_discarded_file_later_without_touching_the_index`.

## 5c. Ramas

`gitcraft-core/src/ops/branch.rs`. Los nombres que llegan de la UI se validan
con `git check-ref-format --branch` (rechaza los que empiezan por «-») y van
siempre detrás de `--`; los puntos de partida son ids o nombres completos
(`refs/…`).

- **Cambiar de rama:** `git switch --no-guess -- <rama>`, nunca forzado. Si Git
  se niega, se calculan con formatos estables los archivos que lo impiden
  (cambios sin confirmar ∩ `git diff --name-only -z HEAD <rama>`) y se
  devuelve `Blocked` sin haber tocado nada. La UI lo explica y ofrece
  «Guardar en el stash y cambiar» (`git stash push` con el mensaje
  «GitCRAFT: antes de cambiar a <rama>», con `--include-untracked` solo si
  algún archivo sin seguimiento estorba) o «Cancelar». Ese stash nunca se
  borra automáticamente.
- **Rama remota:** se crea una rama local con seguimiento
  (`git branch --track`) y se cambia a ella; si ya hay una que la sigue, se
  usa esa.
- **Borrar:** primero `preview_branch_deletion`: los commits que ninguna otra
  rama, rama remota ni tag contiene
  (`rev-list <punta> --not --exclude=<rama> --branches --remotes --tags`).
  El diálogo los lista y muestra `git branch <rama> <punta>` para
  recrearla. `git branch --delete --force` solo se ejecuta tras esa
  confirmación y si la rama sigue en la punta confirmada (si no,
  `BranchChanged` y no se hace nada).

## 5d. Stash

`gitcraft-core/src/ops/stash.rs` y `parse/stash.rs`. Las entradas se
identifican por su commit, no por `stash@{n}` (que cambia al añadir o borrar
entradas): cada operación busca antes el índice actual de ese commit y, si ya
no está, devuelve `StashChanged` sin hacer nada. Las entradas que crea
GitCRAFT se reconocen por su mensaje («GitCRAFT: descarte de …»,
«GitCRAFT: antes de cambiar a …», «GitCRAFT: antes de aplicar …»).

- **Aplicar / pop nunca fuerza.** Antes de `git stash apply`:
  1. Se calculan los archivos que tocaría (`diff` contra `<stash>^1`, índice
     incluido, y los sin seguimiento de `<stash>^3`) y se cruzan con los
     cambios sin confirmar y los archivos existentes. Si coinciden: `Blocked`,
     sin tocar nada. Alternativa: «Guardar mis cambios y aplicar» (stash de lo
     actual, que nunca se borra, y luego aplicar).
  2. Se predicen los conflictos con
     `git merge-tree --write-tree --merge-base=<stash>^1 HEAD <stash>`, que no
     toca nada (Git ≥ 2.40; con versiones anteriores, aproximación por los
     archivos cambiados en ambos lados). Si hay: `WouldConflict`, y solo se
     aplica si el usuario lo acepta (quedan marcas de conflicto).
  3. Pop = aplicar y, **solo si terminó sin conflictos**, `git stash drop` de
     esa entrada.
- **Borrar** siempre tras confirmación, que muestra el hash y
  `git stash store -m "<mensaje>" <hash>` para recuperarla.
- **Contenido:** una entrada es un commit cuyo primer padre es el commit
  sobre el que se creó; su diff contra él son los cambios guardados. Los
  archivos sin seguimiento están en su tercer padre (un commit raíz), así que
  el panel reutiliza los comandos de commits (`commit_changes`,
  `commit_file_diff`). La selección del grafo usa el id `stash:<hash>`.
- **Vigilante:** los cambios en `refs/stash` y `logs/refs/stash` se notifican
  como `stash` (borrar una entrada que no es la última solo reescribe el
  reflog) y la barra lateral recarga la lista.

## 5e. Remotos

`gitcraft-core/src/ops/remote.rs`. Fetch, pull y push se ejecutan con
`AppState::remote_operation`: van por la cola de escritura de la sesión, con
el vigilante en silencio, y con un token de cancelación que
`cancel_operation` activa (el proceso de git y sus hijos —ssh, helpers— se
terminan por grupo). El progreso son las líneas de `--progress` de git
(`GIT_PROGRESS_DELAY=0`), que llegan a la UI por un `Channel`.

- **Credenciales:** solo el credential helper o el agente SSH del usuario.
  Con `GIT_TERMINAL_PROMPT=0` una credencial que falta falla al momento; el
  stderr se clasifica como `authFailed` o `remoteUnreachable` y la UI lo
  explica, con la salida original desplegable.
- **Nombres de remoto** que vienen de la UI: solo se aceptan los
  configurados (`git remote`).
- **Pull nunca fuerza.** Modo: el elegido en GitCRAFT para el repositorio
  (`settings.json`) o, si no, `branch.<rama>.rebase` / `pull.rebase`. Pasos:
  fetch del remoto del upstream → si no hay nada nuevo, «al día» → cambios
  sin confirmar que chocarían (archivos que llegan desde la base común ∩
  cambios locales; con rebase, cualquier cambio en archivos con seguimiento;
  archivos sin seguimiento que llegarían) → `Blocked` sin tocar nada →
  conflictos previstos con `git merge-tree --write-tree HEAD <upstream>` →
  `WouldConflict` sin tocar nada. Solo entonces `git merge --no-edit` o
  `git rebase --no-autostash` (nunca se guarda nada a escondidas). Si el
  usuario acepta los conflictos y git se detiene, el estado
  `in_progress` (marcadores `MERGE_HEAD`, `rebase-merge`… del git dir) se
  muestra en un aviso con «Abortar» (`git merge --abort` /
  `git rebase --abort`).
- **Push:** `git push --porcelain --progress <remoto> refs/heads/<rama>:<rama
  remota>` (refs completas, nunca opciones). El rechazo se lee de la salida
  `--porcelain` (líneas con `!`): «fetch first» / «non-fast-forward» → hay
  que hacer pull; «stale info» → el lease caducó. Nunca `--force`: el push
  forzado es `--force-with-lease=<rama remota>:<commit>` con el commit que
  mostró la confirmación (`force_push_preview`, tras un fetch).
- **Pruebas:** `scripts/make-remote-sandbox.sh` crea un repo de trabajo, un
  remoto bare y un segundo clon («otra persona»); los tests de integración
  hacen lo mismo con `TestRepo::add_bare_remote` y `TestRepo::clone_of`.

## 5f. Crear repositorios

- **Inicializar** (`ops/init.rs`): `check_init` dice antes qué pasaría (rama
  propuesta `init.defaultBranch` o «main», si ya es un repositorio, si
  quedaría anidado en otro, si la carpeta está vacía). Nunca reinicializa un
  repositorio existente; los archivos que haya quedan sin seguimiento.
- **Clonar** (`ops/clone.rs`): `git clone --progress -- <url> <destino>`. El
  destino tiene que no existir o ser una carpeta vacía (`check_destination`,
  también al escribir en la UI). La cancelación tiene su propio token en
  `AppState` (no hay sesión todavía); git borra lo que hubiera creado.

## 6. Persistencia

`settings.json` en el `app_config_dir` de Tauri (en macOS,
`~/Library/Application Support/dev.gitcraft.app/`) guarda:

- Los recientes: máximo 20 y se descartan los que ya no existen.
- Las pestañas abiertas (rutas en orden) y la activa. La UI las guarda en
  cada cambio (`save_session`), solo después de haber restaurado la sesión
  anterior (`restore_session`), para que un arranque lento no la borre.
  Restaurar no reordena los recientes; un repo que ya no existe o no es
  válido no se abre, se avisa una vez y se quita de la sesión guardada.
  La pestaña nueva (⌘T, «+») no se guarda. Es una sola, siempre la última,
  y muestra la pantalla de inicio. Está activa cuando `activeId` es `null`,
  y al abrir, clonar o inicializar un repo desde ella se convierte en la
  pestaña de ese repo. Mientras está activa, la sesión guarda como activa la
  pestaña de su izquierda.
- Las preferencias: modo de pull por repo y los ajustes generales
  (`general`): idioma (`null` = el del sistema), tamaño de texto, carpeta
  por defecto para clonar y fetch automático. El tamaño de texto se aplica
  con el zoom del webview (`setZoom`, permiso
  `core:webview:allow-set-webview-zoom`). Lo que pertenece a git (identidad,
  rama inicial, estrategia de pull por defecto) **no** se guarda aquí: se
  lee y se escribe en la configuración de git.
- **Configuración de git** (`gitcraft_core::ops::config`): `get` (`-z`),
  `effective` (`--show-scope --show-origin -z`) y `apply`, que escribe con
  `git config --global|--local -- <clave> <valor>` y quita con
  `--unset-all` (el código 5, «no estaba», cuenta como éxito). Las claves
  las fija GitCRAFT. La app expone comandos concretos (rama inicial,
  estrategia de pull y, en D.3, identidad), nunca un «escribir clave»
  genérico. Para aislar los tests, `Git::with_env` fija
  `GIT_CONFIG_GLOBAL` en cada comando.
- **Fetch automático** (`src/features/remote/autoFetch.ts`): un intervalo
  en el frontend recorre las pestañas abiertas una tras otra. Usa la misma
  operación remota (con progreso y cancelación) y nunca se lanza si ya hay
  otra en curso. Los fallos se guardan en `autoFetchErrors` y se marcan
  junto a la rama.

Se escribe de forma atómica (archivo temporal + rename). Si el archivo está
corrupto, se usan los valores por defecto. **Nunca** guarda credenciales.

### Errores hacia la UI

Los errores llegan como `IpcError { kind, message, details }`:

- `kind` (`ErrorKind`) lo clasifica el core a partir del stderr, que con
  `LC_ALL=C` es estable, y elige el mensaje traducido `errors.<kind>`.
- `details` contiene el comando y la salida original de git, que la UI muestra
  bajo demanda.

**Catálogo (`ErrorKind`).** Cada tipo tiene un mensaje en `errors.<kind>` que
dice qué pasa y qué hacer:

| Grupo | Tipos | Origen |
|---|---|---|
| Entorno | `gitNotFound`, `pathNotFound`, `timedOut`, `diskFull`, `filePermission`, `indexLocked` | Arranque del proceso, rutas, stderr |
| Repositorio | `notARepository`, `bareRepository`, `dubiousOwnership`, `alreadyARepository`, `destinationNotEmpty` | stderr y comprobaciones propias |
| Commit | `identityUnknown`, `nothingToCommit`, `emptyMessage` | stderr/stdout de `git commit` |
| Cambios locales | `staleDiff`, `undoConflict`, `localChangesOverwritten`, `nothingToStash`, `stashChanged` | Comprobaciones propias y stderr |
| Ramas | `invalidBranchName`, `branchExists`, `branchChanged`, `branchInWorktree`, `detachedHead` | `check-ref-format`, comprobaciones propias, stderr |
| Remotos | `authFailed`, `remoteUnreachable`, `remoteNotFound`, `notFastForward`, `unrelatedHistories` | stderr de fetch/pull/push/clone |
| Otros | `cancelled` (no se muestra como error), `unknown` | — |

`unknown` también tiene explicación: dice que GitCRAFT no reconoce el error y
remite a la salida original, que siempre existe (si git no la dio, se usa el
mensaje técnico). Los mensajes que nombran una carpeta (`{{path}}`) tienen una
variante `_noPath` para cuando no hay ruta que nombrar.

**Un único componente: `src/ui/ErrorReport.tsx`.** Muestra qué se intentaba
(`lead`), la explicación del catálogo, notas propias del contexto y, si hay
salida, un desplegable «Ver la salida original de Git» con el texto
seleccionable y el botón «Copiar la salida». Un test comprueba que todo
`ErrorKind` tiene mensaje en cada idioma (la lista `ERROR_KINDS` la mantiene
completa el compilador).

Dónde se muestra:

| Lugar | Errores |
|---|---|
| Banner superior (`OpenErrorBanner`) | Abrir repositorio, reabrir la sesión anterior (cada pestaña, o la sesión entera) |
| Historial (`GraphView`, `OptimizeNotice`) | Cargar el historial, optimizar |
| Panel de detalle (`DetailPanel`, `FileList`, `DiffView`) | Cargar commit, archivos, diff |
| Cambios locales (`WorkingDirPanel`, `CommitBox`, `Discard`) | Leer el estado, stage/unstage/commit, activar amend, deshacer descarte |
| Avisos de ramas, stash y remotos (`BranchToast`, `StashToast`, `RemoteToast`) | Toda operación de ramas, stash, fetch/pull/push, guardar el modo de pull |
| Diálogos de crear (`CreateDialogs`) | Inicializar, comprobar el destino del clon, clonar |

Solo se ignoran sin aviso fallos que no impiden nada y se reintentan solos
(cargar la lista de recientes, recordar «no mostrar más» el aviso de
optimizar, sugerir el nombre de la carpeta del clon); las comprobaciones
previas que fallan dejan seguir y el error de la operación real se explica.

### Panel de operaciones e historial de comandos

- **Registro en el núcleo.** `Git::with_log(Arc<dyn CommandLog>)` hace que
  cada `GitCommand` avise al empezar (`CommandEvent::Started`: id, carpeta,
  línea de comando, lectura/escritura, hora) y al terminar
  (`Finished`: duración y `CommandOutcome`: correcto, código de salida,
  cancelado, tiempo agotado, no se pudo ejecutar). Si el futuro del comando se
  abandona antes de terminar, se registra como cancelado.
- **Comando exacto.** La línea es la que se ejecuta: `git`, los `-c` fijos y
  los argumentos, con comillas de shell solo donde hacen falta, de modo que se
  puede copiar y pegar. No incluye el entorno fijo (`LC_ALL=C`,
  `GIT_TERMINAL_PROMPT=0`…) ni la entrada estándar.
- **Sin credenciales.** `redact::redact_credentials` sustituye por `***` la
  parte usuario/contraseña de toda URL (`https://***@host/…`) y el valor de
  parámetros como `token`, `access_token` o `password`. Se aplica a la línea
  del historial y también a `details` y `message` de los errores. Nunca se
  guarda la salida de los comandos en el historial.
- **App.** `CommandHistory` (`src-tauri/src/history.rs`) guarda en memoria los
  últimos 1000 comandos (no se persisten) y emite el evento `CommandRecord`
  con cada inicio y fin. Comandos: `command_history` y
  `clear_command_history`.
- **UI.** `src/features/operations/`: el botón «Operaciones» de la barra
  superior abre un panel inferior con «En curso» (la operación remota con su
  progreso y «Cancelar», y los comandos que siguen en marcha con su tiempo) y
  el historial, del más reciente al más antiguo. Filtros: solo la pestaña
  activa (por carpeta) y ocultar las lecturas.

### Empaquetado (macOS)

- `npm run dist` → `tauri build --bundles app,dmg` (con `CI=true`, para que el
  `.dmg` se cree sin automatizar Finder). Compila para la arquitectura del Mac
  que lo ejecuta (arm64 en Apple Silicon); salida en
  `target/release/bundle/{macos,dmg}`. Instalación y primer arranque:
  `docs/INSTALACION.md`.
- **Firma ad-hoc** (`signingIdentity: "-"`): no identifica a ningún
  desarrollador ni está notarizada, así que Gatekeeper la trata como app sin
  firmar. Hace falta porque en Apple Silicon macOS solo ejecuta código con
  alguna firma, y un `.app` sin sellar se reporta como «dañado» al
  descargarlo.
- **`minimumSystemVersion: "11.0"`**, la primera versión de macOS para Apple
  Silicon. Tauri lo pasa como `MACOSX_DEPLOYMENT_TARGET` a toda la
  compilación, incluidos los proc-macros. Con el toolchain actual en macOS 27,
  cualquier valor ≥ 12.0 genera dylibs de proc-macro que rustc no puede
  cargar (`E0463: can't find crate for …_macros`). Hay que volver a comprobarlo
  antes de subir el mínimo.
- **PATH de git.** Una app abierta desde Finder recibe un `PATH` mínimo.
  `find_git` ya busca en las rutas de Homebrew, y además el runner añade al
  final del `PATH` de git `/opt/homebrew/bin`, `/usr/local/bin` y las rutas
  del sistema que falten. Así git encuentra sus helpers: credential helpers
  como `gh`, git-lfs o ssh.

### Tokens de diseño y contraste

Todos los colores están en `src/ui/tokens.css`: superficies, texto, acento,
estados, carriles del grafo y derivados (`--selected-bg`, tintes del diff,
sombras…). Los componentes solo usan `var(--…)`.
`src/ui/tokens.test.ts` falla si aparece un color literal (`#…`, `rgb()`,
`color-mix()`…) en cualquier otro archivo de `src/`. Hay una sola excepción:
el tinte de las etiquetas de rama, que mezcla `var(--lane)`, una variable que
se fija en cada etiqueta. El canvas del grafo lee los tokens con
`getComputedStyle`, porque no puede usar variables CSS.

- **Superficies:** de `--surface-0` (fondo) a `--surface-3` (*hover* y
  controles elevados).
  - `--border` separa zonas y es decorativo.
  - `--border-strong` es el borde de menús, tooltips, diálogos y etiquetas.
  - `--border-control` es el contorno de los controles interactivos (botones,
    campos, barra de progreso).
- **Tintes translúcidos** (`--accent-tint` y los fondos del diff): solo sobre
  `--surface-0` a `--surface-2`.
- **Texto seleccionado:** siempre en `--text`, sea cual sea el color del texto
  original.

El test calcula los ratios WCAG a partir de los propios tokens, resolviendo
las variables y `color-mix` (con alfa premultiplicado). En los fondos
interactivos (las superficies, `--selected-bg` y `--selected-inactive-bg`)
comprueba también el aspecto con *hover* y pulsado. El peor caso es el
mínimo actual:

| Pares | Mínimo exigido | Peor caso actual |
| --- | --- | --- |
| `--text` sobre todas las superficies, fondos de estado y tintes | 4,5:1 | 8,92 |
| `--text-muted` (ídem) | 4,5:1 | 5,79 |
| `--text-faint` (ídem) | 4,5:1 | 4,65 |
| `--accent`, `--danger`, `--success`, `--warning`, `--info` como texto (ídem) | 4,5:1 | 4,51 (`--danger`) |
| `--text` sobre la selección de texto | 4,5:1 | 5,71 |
| `--accent-contrast` sobre `--accent` y `--accent-strong` (en todos sus estados) y sobre cada carril | 4,5:1 | 4,63 |
| `--danger-contrast` sobre `--danger` (en todos sus estados) | 4,5:1 | 4,72 |
| `--border-control` sobre los fondos interactivos | 3:1 | 3,14 |
| `--focus-ring` (ídem) | 3:1 | 4,59 |
| Carriles sobre `--surface-0` y los fondos de selección | 3:1 | 4,83 |

### Estados de interacción

Son reglas comunes en `tokens.css`: ningún componente las redefine.

- **Hover y pulsado:** una capa translúcida en `background-image`
  (`--hover-layer`, que aclara un 4 %, y `--pressed-layer`, que oscurece un
  22 %). Se pinta sobre el fondo de cualquier botón, pestaña, opción, elemento
  de menú o fila. Por eso las reglas de estado de los componentes usan
  `background-color` y nunca el atajo `background`, que borraría la capa; el
  test lo comprueba.
- **Seleccionado:** `--selected-bg` más una marca lateral de 2 px
  (`--selected-marker`), que no depende solo del color. Cuando el foco está en
  otra zona, el fondo pasa a `--selected-inactive-bg` y la marca se mantiene.
  El historial es enfocable (`tabIndex=0`) para que se note cuándo tiene el
  foco. La selección se expone con `aria-selected`, `aria-current` o
  `aria-pressed`, según el control.
- **Foco de teclado:** anillo de 2 px en `--focus-ring`, solo con
  `:focus-visible`. Las filas y pestañas lo dibujan por dentro para que el
  scroll no lo recorte. Si una regla quita el `outline`, debe dar otra señal
  visible (fondo, borde o sombra); el test lo comprueba.
- **Deshabilitado:** todo el control se atenúa (`--disabled-opacity`) y deja
  de reaccionar al *hover* y al pulsado. Los botones rellenos además pierden
  el relleno y quedan solo con el contorno, así que el cambio no depende solo
  del color.
- **Tooltips** (`src/ui/Tooltip.tsx`): justo antes de mostrarse se miden y
  se colocan con `placeTooltip`. Se desplazan en horizontal para quedar a
  8 px del borde de la ventana, y aparecen encima del control si debajo no
  caben y hay más sitio arriba.
- **Menús de un botón** (`MoreOptionsButton`): un chevron dibujado en SVG,
  en un botón cuadrado con la altura de los de la barra (28 px), y el
  tooltip «Más opciones de …».

Solo hay tema oscuro; el tema claro está en la fase 3.

## 7. Estrategia de tests

| Nivel | Qué | Cómo |
| --- | --- | --- |
| Unitario (core) | Parsers, layout del grafo, generación de parches, clasificador de errores | Fixtures con salida real de git + snapshots `insta` |
| Integración (core) | `ops::*` contra repos reales | `TestRepo::new()` en `tempfile`, `GIT_CONFIG_GLOBAL=/dev/null`, fechas e identidad fijas |
| Rendimiento | Log + layout de 100k commits | Test `#[ignore]` y bench con un repo sintético generado por `git fast-import` |
| Frontend | Componentes y stores | Vitest + Testing Library, IPC mockeado |
