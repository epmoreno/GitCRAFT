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
  `RepoSession` para evitar choques con `index.lock`.
- **Vigilante de `.git` y del worktree** (`src-tauri/src/watcher.rs`, crate
  `notify`). Clasifica cada cambio como `refs` (HEAD, `packed-refs`,
  `refs/**` del git dir y del common dir → se recarga el grafo) o `worktree`
  (archivos, `index`, `MERGE_HEAD`… → se recarga el estado). Agrupa eventos con
  un debounce de
  ~300 ms. Mientras haya una operación propia en curso en ese repo, sus eventos
  se **ignoran**; al terminar la operación se emite un único `repo-changed`.
  Así se evitan los refrescos en bucle (por ejemplo, un `status` que toca el
  índice y vuelve a disparar el watcher).
- **Errores:** `GitError { kind: ErrorKind, args, exit_code, stderr, stdout_tail }`.
  Un clasificador con patrones (con `LC_ALL=C` los mensajes son estables) los
  asigna a `AuthFailed`, `NonFastForward`, `MergeConflict`,
  `WouldOverwriteLocalChanges`, `IndexLocked`, `NotARepository`, `HookFailed`,
  `Unknown`… El frontend muestra `t("errors.<kind>")` y un desplegable con la
  salida original.
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
   con una paleta propia que rota.
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

## 6. Persistencia

`settings.json` en el `app_config_dir` de Tauri (en macOS,
`~/Library/Application Support/dev.gitcraft.app/`) guarda:

- Los recientes: máximo 20 y se descartan los que ya no existen.
- Las pestañas abiertas.
- Las preferencias: modo de pull por repo, tema e idioma.

Se escribe de forma atómica (archivo temporal + rename). Si el archivo está
corrupto, se usan los valores por defecto. **Nunca** guarda credenciales.

### Errores hacia la UI

Los errores llegan como `IpcError { kind, message, details }`:

- `kind` (`ErrorKind`) lo clasifica el core a partir del stderr, que con
  `LC_ALL=C` es estable, y elige el mensaje traducido `errors.<kind>`.
- `details` contiene el comando y la salida original de git, que la UI muestra
  bajo demanda.

## 7. Estrategia de tests

| Nivel | Qué | Cómo |
| --- | --- | --- |
| Unitario (core) | Parsers, layout del grafo, generación de parches, clasificador de errores | Fixtures con salida real de git + snapshots `insta` |
| Integración (core) | `ops::*` contra repos reales | `TestRepo::new()` en `tempfile`, `GIT_CONFIG_GLOBAL=/dev/null`, fechas e identidad fijas |
| Rendimiento | Log + layout de 100k commits | Test `#[ignore]` y bench con un repo sintético generado por `git fast-import` |
| Frontend | Componentes y stores | Vitest + Testing Library, IPC mockeado |
