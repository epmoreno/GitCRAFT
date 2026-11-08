# GitCRAFT — Visión

## Objetivo

**GitCRAFT – cliente Git visual, rápido y de código abierto.** Hace que el
historial y las operaciones de Git se entiendan de un vistazo y se ejecuten con
pocos clics, sin esconder nunca lo que Git hace por debajo.

- **Plataforma:** primero macOS en Apple Silicon. Después Windows y Linux.
- **Sin IA:** no hay ninguna función de IA integrada, ni ahora ni como función
  opcional.
- **Licencia:** MIT.

## Principios de producto

1. **El grafo es el centro.** El historial de commits, con carriles de color por
   rama y etiquetas de ramas, tags y HEAD, es la vista principal. Tiene que ir
   fluido con decenas de miles de commits.
2. **Feedback inmediato.** Cada operación muestra progreso, se puede cancelar si
   es larga y deja claro cómo ha quedado el repositorio.
3. **Seguro por defecto.** Toda operación destructiva (reset --hard, borrar rama,
   descartar cambios, force push…) pide confirmación y explica en lenguaje claro
   qué va a pasar y qué se pierde. Si se puede deshacer de forma segura, se ofrece.
4. **Errores comprensibles.** Los errores de Git se traducen a un mensaje claro y
   siempre se puede ver la salida original de `git`.
5. **Respeta el entorno del usuario.** Usa el binario `git` del sistema, y con él
   la configuración, los hooks, el credential helper, las claves SSH y la firma
   del usuario. GitCRAFT no guarda credenciales propias.
6. **Información densa y limpia.** Diseño propio, compacto y legible. Modo oscuro
   por defecto.
7. **Ligero.** Arranque rápido, poco consumo de memoria y un binario pequeño.

## Alcance

### MVP
1. Abrir, clonar e inicializar repositorios; pestañas con varios repos abiertos;
   lista de recientes.
2. Grafo de commits: carriles de colores, etiquetas de ramas, tags y HEAD;
   virtualización y carga incremental.
3. Detalle de commit: autor, fecha, mensaje, archivos cambiados y diff.
4. Working directory: unstaged y staged; stage y unstage por archivo, hunk y
   línea; caja de commit (resumen + descripción); amend.
5. Ramas locales y remotas: crear, checkout, renombrar y borrar.
6. Remotos: fetch, pull (merge o rebase configurable), push y push con upstream;
   indicadores de commits por delante y por detrás.
7. Stash: guardar, aplicar, pop, borrar y ver contenido.

### Fase 2
Merge y rebase arrastrando ramas, resolución de conflictos a tres paneles,
cherry-pick, revert, reset, tags, deshacer y rehacer basados en reflog, búsqueda
en el historial, historial y blame de un archivo, y rebase interactivo visual.

### Fase 3
Submódulos, worktrees, Git LFS, firma GPG/SSH, perfiles de identidad, atajos
configurables y tema claro/oscuro.

### Fuera de alcance
- Funciones de IA de cualquier tipo.
- Alojar repositorios o integrarse con las APIs de proveedores (PRs, issues).
  La única excepción es publicar un repositorio local en GitHub mediante `gh`
  (GitHub CLI).
- Una implementación propia del protocolo Git o del almacenamiento de
  credenciales.

## Reglas de origen (clean-room)

GitCRAFT es una implementación **100 % clean-room**. Estas reglas no se pueden
negociar:

1. **Referencia solo de experiencia de usuario.** Los clientes Git propietarios
   sirven de referencia **solo** para ideas generales de experiencia de usuario, como el grafo central, las
   operaciones con clic o arrastrar y soltar, y el feedback inmediato.
2. **Ingeniería inversa (REA) solo sobre objetivos permitidos.** Se usa solo a
   petición del usuario, o cuando se propone y el usuario lo aprueba.
   - *Permitidos:* GitHub Desktop (MIT) y SourceGit (MIT), para entender cómo
     implementan una función concreta (si se reutiliza código, se conserva su
     aviso MIT y se registra en `THIRD_PARTY_NOTICES.md`, sin usar sus nombres,
     logos ni recursos gráficos); y los builds propios de GitCRAFT, para
     depuración, crashes y verificación.
   - *Prohibidos:* cualquier software propietario cuya licencia no permita su
     análisis. De ellos solo se usa el comportamiento observable como usuario,
     descrito con palabras propias.
   - Antes de analizar cualquier otra aplicación se verifica su licencia y se
     pregunta al usuario.
3. **Identidad propia.** No se copian nombres, logos, iconos, paletas de color,
   textos, tipografías propietarias ni recursos gráficos de ningún producto.
4. **Open source con licencia compatible.** Se puede consultar el código de
   clientes Git con licencia compatible, como SourceGit, GitHub Desktop o lazygit
   (MIT), para resolver dudas de implementación. Si se reutiliza código, se
   respeta su licencia, se conserva su aviso de copyright y se cita en
   `THIRD_PARTY_NOTICES.md` y en un comentario junto al código.
5. **Documentación pública.** La documentación oficial de Git (`git help`,
   git-scm.com) y las especificaciones públicas son la fuente principal para
   entender los formatos de salida.
