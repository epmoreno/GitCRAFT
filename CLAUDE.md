# CLAUDE.md — GitCRAFT

Guía de trabajo para el desarrollo asistido por IA de este proyecto. GitCRAFT,
la aplicación, no incluye ninguna función de IA.

Cliente Git de escritorio original (Tauri 2 + Rust + React + TypeScript).
Visión y alcance: `docs/VISION.md`. Tareas: `docs/ROADMAP.md`.
Arquitectura: `docs/ARCHITECTURE.md`.

## Identidad pública

Descripción oficial: «GitCRAFT – cliente Git visual, rápido y de código abierto» (en inglés: «GitCRAFT – a visual, fast, open-source Git client»).

En textos públicos no se menciona ningún otro cliente Git; la descripción
oficial es la de arriba. Sí se nombran las herramientas con las que GitCRAFT se
integra y los avisos de licencia.

## Idiomas de la documentación

La documentación pública existe en inglés y español en archivos separados;
cada cambio se hace en los dos idiomas en el mismo commit. Hoy son
`README.md`/`README.es.md` y `docs/INSTALL.md`/`docs/INSTALACION.md`, cada uno
con la barra de idiomas al principio. Un archivo no mezcla idiomas. La
documentación de desarrollo (`docs/VISION.md`, `docs/ROADMAP.md`,
`docs/ARCHITECTURE.md`, `docs/benchmarks.md`) sigue solo en español.

## Reglas clean-room (obligatorias)

- Los clientes propietarios son referencia **solo** de ideas generales de UX. No
  se copian nombres, logos, iconos, paletas, textos ni recursos gráficos.
- Se puede consultar código open source con licencia compatible (SourceGit,
  GitHub Desktop, lazygit: MIT). Si se reutiliza código: se conserva el aviso de
  copyright, se añade una entrada a `THIRD_PARTY_NOTICES.md` y un comentario
  `// Adapted from <proyecto> (<licencia>): <url>` junto al código.
- Sin funciones de IA en el producto.

## Uso de REA (reverse engineering)

**Solo cuando el usuario lo pida, o cuando se proponga y el usuario lo apruebe.**

Objetivos permitidos:

- **GitHub Desktop (MIT) y SourceGit (MIT):** para entender cómo implementan una
  función concreta. Si se reutiliza código, se conserva el aviso de licencia MIT
  y se registra el origen en `THIRD_PARTY_NOTICES.md`. No se usan sus nombres,
  logos ni recursos gráficos.
- **Builds propios de GitCRAFT:** depuración, crashes y verificación.

Objetivos prohibidos: **cualquier software propietario cuya licencia no permita
su análisis.** De ellos solo se usa el comportamiento observable como usuario,
descrito con palabras propias.

Antes de analizar cualquier otra app: verificar su licencia y preguntar al
usuario.

## Seguridad y Git

- `git` se invoca **siempre** con `std::process::Command` / `tokio::process::Command`
  y los argumentos como array. **Nunca** se usa shell (`sh -c`) ni se interpolan
  entradas del usuario en una cadena de comando.
- Las entradas del usuario que puedan empezar por `-` (nombres de rama, rutas,
  refs) se separan con `--` o se validan (`git check-ref-format`).
- No se guardan credenciales. La autenticación la hace el credential helper o SSH
  del usuario. Se ejecuta con `GIT_TERMINAL_PROMPT=0` y los fallos de
  autenticación se muestran como error claro.
- Toda la salida que se parsea usa formatos estables: `--porcelain=v2`, `-z`,
  `--format` con separadores `%x00`/`%x1f`. Nunca se parsea salida pensada para
  humanos. `LC_ALL=C` para que los mensajes de error sean estables.
- Toda operación destructiva pasa por un diálogo de confirmación que explica sus
  consecuencias.

## Convenciones

- **Idioma:** código, identificadores, comentarios y mensajes de commit en
  **inglés**. Documentación de `docs/` en **español**. Textos de la UI siempre
  mediante i18n (`src/i18n/{es,en}.json`), nunca literales en componentes.
- **Multiplataforma:** todo código nuevo debe ser multiplataforma (macOS,
  Windows y Linux); lo específico de un sistema va detrás de una abstracción
  y se apunta en el inventario de `docs/ARCHITECTURE.md` (sección 8).
- **Rust:** edition 2024, `cargo fmt`, `cargo clippy -- -D warnings`. Errores con
  `thiserror` en las librerías; nada de `unwrap()` fuera de tests. Lógica Git en
  el crate `gitcraft-core` (sin dependencias de Tauri) para poder testearla
  aislada. `unsafe` está prohibido (`unsafe_code = "deny"`) salvo en módulos de
  plataforma (p. ej. `windows_trash.rs`), con `#[allow(unsafe_code)]` acotado
  a ese módulo o función y un comentario `SAFETY` que lo justifique en cada
  bloque; nunca en la lógica de la app.
- **TypeScript:** `strict`, ESLint + Prettier. Los tipos de los comandos Tauri se
  generan desde Rust (`specta`/`ts-rs`), no se escriben a mano.
- **Tests:**
  - Unitarios para cada parser, con salidas reales de git guardadas como
    fixtures (`crates/gitcraft-core/tests/fixtures/`).
  - Integración contra repos temporales creados en el propio test (`tempfile`) con
    identidad y fechas fijas (`GIT_AUTHOR_DATE`, `GIT_COMMITTER_DATE`), aislados
    de la config global (`GIT_CONFIG_GLOBAL` al dispositivo nulo del sistema,
    `testing::NO_CONFIG`, y `GIT_CONFIG_NOSYSTEM=1`).
  - Frontend: Vitest + Testing Library.
- **Commits:** directamente a `main`, un commit por paso del roadmap, mensaje
  en inglés estilo Conventional Commits (`feat(graph): …`, `fix(status): …`).
  Cada commit compila y pasa `npm run check`.
  **Nunca se hace commit si `npm run check` no ha pasado:** se ejecuta siempre
  como `npm run check && git commit …`.
- **Comandos:** `npm run tauri dev` (app), `npm run check` (lint + tests),
  `cargo test -p gitcraft-core` (tests del núcleo).

## Flujo de trabajo

1. Pasos pequeños siguiendo `docs/ROADMAP.md`. Al terminar un paso se marca la
   casilla, se hace commit y se explica qué se ha hecho, cómo probarlo y cuál es
   el siguiente.
2. Los cambios de arquitectura se reflejan en `docs/ARCHITECTURE.md`.
3. **Verificación manual:** algo solo se marca como verificado (en el roadmap,
   en docs o en un commit) cuando el usuario confirma que lo ha probado, y se
   describe exactamente lo que probó (p. ej. «verificado manualmente con un repo
   grande»), sin añadir detalles que no ha dado. Los tests automáticos no
   cuentan como verificación manual.
4. **Instrucciones de prueba seguras:** nunca se proponen comandos que puedan
   borrar cambios sin commitear (`git reset --hard`, `git checkout -- .`,
   `git clean -f`, `git restore` sobre el worktree…). Para deshacer un commit
   de prueba se usa `git reset --soft HEAD~1`, y las pruebas se hacen en un
   repositorio de prueba desechable, no en GitCRAFT.
5. **Procesos que no terminan solos:** nunca se ejecutan en primer plano
   procesos que no terminan solos (`tauri dev`, servidores, watchers). Se
   lanzan en segundo plano con un límite de tiempo, se revisa su salida y se
   terminan. Si una comprobación tarda más de 2 minutos sin salida nueva, se
   detiene y se avisa al usuario.
6. **Datos personales:** nunca se escriben en archivos del repo datos
   personales (emails reales, rutas del Mac del usuario, nombres de usuario
   del sistema), aunque aparezcan en los mensajes del usuario. El mailmap y
   cualquier archivo con el email antiguo van fuera del repo.
7. **Autenticación y portapapeles:** nunca se lanzan flujos de autenticación
   (`gh auth login`, credenciales, llavero) ni se escribe en el portapapeles
   del sistema sin pedir permiso antes al usuario, aunque sea en un entorno
   aislado.
