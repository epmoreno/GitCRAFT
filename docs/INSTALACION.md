# Instalar GitCRAFT

🌐 [English](INSTALL.md) · **Español**

[macOS](#macos-apple-silicon) · [Windows](#windows) · [Linux](#linux)

## macOS (Apple Silicon)

GitCRAFT se distribuye como `.dmg` para Macs con chip Apple (M1 o posterior).
**Probado en macOS 27.0.1; versiones anteriores no verificadas.** **No está
firmado con un certificado de desarrollador de Apple ni notarizado** (más
abajo, [por qué](#por-qué-gitcraft-no-está-firmado)): solo lleva una firma
*ad-hoc* (local, sin identidad), que en Apple Silicon es necesaria para que
macOS acepte ejecutar la app. Por eso la primera vez que la abras, macOS te
pedirá confirmación. No hace falta desactivar Gatekeeper.

### Requisitos

- Git instalado: el de las Command Line Tools de Xcode
  (`xcode-select --install`) o el de Homebrew (`brew install git`). GitCRAFT
  no incluye su propio Git.
- Git 2.30 o posterior. En el rebase interactivo, editar mensajes y combinar
  commits necesitan Git 2.32 o posterior, y prever exactamente los conflictos,
  2.40 o posterior; con un Git más antiguo, GitCRAFT explica qué no está
  disponible. Para ver qué versión tienes: `git --version` en el Terminal.
- Para trabajar con remotos privados: un credential helper (en macOS,
  `osxkeychain`, que viene con Git) o una clave SSH en el agente. GitCRAFT
  nunca guarda ni pide contraseñas.
- Para la cuenta de GitHub y «Publicar en GitHub»: GitHub CLI 2.81.0 o
  posterior (`brew install gh`; si ya lo tienes, `brew upgrade gh`). Para ver
  tu versión: `gh --version`. GitCRAFT no guarda tokens; la sesión la guarda gh en
  el Llavero.

### Generar el `.dmg`

```sh
npm install        # una vez
npm run dist
```

Hay que ejecutarlo en un Mac con chip Apple: compila para la arquitectura del
equipo (arm64). El resultado queda en
`target/release/bundle/dmg/GitCRAFT_<versión>_aarch64.dmg` (o bajo
`$CARGO_TARGET_DIR` si está definido).

### Instalar

1. Descarga el `.dmg` de
   [Releases](https://github.com/epmoreno/GitCRAFT/releases) o genéralo tú
   (arriba).
2. Ábrelo con doble clic.
3. Arrastra **GitCRAFT** a la carpeta **Aplicaciones**.
4. Expulsa el disco «GitCRAFT» (botón ⏏ en la barra lateral de Finder).

### Abrir por primera vez

**Si el `.dmg` lo generaste tú en este Mac**, normalmente se abre sin aviso:
macOS solo pide confirmación para lo que llega descargado (navegador,
AirDrop, correo, mensajería…).

**Si lo descargaste o te lo pasaron**, macOS lo bloquea la primera vez
porque no puede comprobar quién lo ha hecho:

1. Abre GitCRAFT desde Aplicaciones (doble clic). Aparece un aviso de que no
   se ha podido verificar; pulsa **Aceptar** (o **Hecho**). No elijas «Trasladar
   a la Papelera».
2. Abre **Ajustes del Sistema → Privacidad y seguridad** y baja hasta la
   sección **Seguridad**. Verás un mensaje sobre «GitCRAFT» con el botón
   **Abrir igualmente**. Púlsalo.
3. Confirma con tu contraseña o Touch ID y pulsa **Abrir** en el último
   aviso.

Solo hay que hacerlo una vez: macOS recuerda la excepción **solo para esta
app**. Gatekeeper sigue activo para todo lo demás.

No hagas nada de esto:

- `sudo spctl --master-disable` (o «Permitir apps de: Cualquier sitio»):
  desactiva la protección para todas las apps.
- Quitar atributos con `xattr` para saltarte el aviso: el paso de Ajustes
  hace lo mismo para esta app, de forma visible y reversible.

### ¿Por qué GitCRAFT no está firmado?

GitCRAFT es software libre y gratuito (licencia MIT): no se cobra nada por él.
Firmarlo y notarizarlo con Apple exige pagar una cuota anual del programa de
desarrolladores de Apple, y el proyecto ha decidido no hacerlo. Por eso macOS
muestra un aviso la primera vez que se abre: no puede comprobar quién ha hecho
la app.

Para abrirla, sigue los pasos de [Abrir por primera vez](#abrir-por-primera-vez):
abre la app, pulsa **Aceptar** en el aviso y luego **Ajustes del Sistema →
Privacidad y seguridad → Abrir igualmente**. **No desactives Gatekeeper:**
la excepción es solo para GitCRAFT.

#### Cómo comprobar la app

Sin firma de Apple, puedes comprobar tú mismo lo que descargas:

- **Checksum SHA-256.** Las notas de cada Release en GitHub incluyen el
  SHA-256 del `.dmg`. Calcula el del archivo descargado y compáralo; tienen
  que ser idénticos:

  ```sh
  shasum -a 256 ~/Downloads/GitCRAFT_<versión>_aarch64.dmg
  ```

- **VirusTotal.** Las notas de cada Release enlazan el análisis de ese mismo
  `.dmg` en VirusTotal. Comprueba que el SHA-256 que muestra VirusTotal es el
  de tu archivo.
- **Compilarlo tú.** El código es público: puedes generar el `.dmg` desde el
  código fuente con `npm run dist` (ver [Generar el `.dmg`](#generar-el-dmg))
  y no descargar nada. Un `.dmg` compilado por otra persona tiene otro SHA-256
  (lleva fechas de compilación y una firma *ad-hoc*), así que compilar es una
  alternativa a descargar, no una forma de comprobar un archivo descargado.

### Desinstalar

1. Cierra GitCRAFT (⌘Q).
2. Arrastra **GitCRAFT** desde Aplicaciones a la Papelera.
3. Opcional, para borrar también sus ajustes (recientes, pestañas abiertas,
   modo de pull por repositorio, ajustes generales y vista del panel de
   cambios): en Finder, **Ir → Ir a la carpeta…**
   (⇧⌘G) y lleva a la Papelera, si existen:
   - `~/Library/Application Support/dev.gitcraft.app`
   - `~/Library/Caches/dev.gitcraft.app`
   - `~/Library/WebKit/dev.gitcraft.app`

Desinstalar GitCRAFT **no toca tus repositorios** ni la configuración de Git:
GitCRAFT no guarda nada dentro de ellos, salvo lo que tú hagas con Git (commits,
ramas, stashes…).

Tampoco deshace lo que hayas cambiado desde Ajustes en la configuración de
git o en gh, porque eso es configuración tuya y sigue sirviendo en la
terminal:

- identidad, rama inicial y estrategia de pull (`~/.gitconfig` o el
  `.git/config` de cada repo);
- el credential helper de gh, si lo activaste;
- la sesión de gh.

## Windows

> **Todavía no publicado:** el instalador llega con la v0.4.1.

Para Windows 11, en x64 y ARM64. **No está firmado** (más abajo,
[por qué](#por-qué-no-está-firmado-en-windows)), así que Windows avisa antes
de abrirlo. No hace falta desactivar ninguna protección. El de ARM64 solo se
prueba automáticamente, en el CI.

**Qué necesita:** [Git para Windows](https://git-scm.com/downloads/win)
(`winget install --id Git.Git`), 2.30 o posterior; GitCRAFT no incluye su
propio Git. `gh` es opcional, para las funciones de GitHub: GitHub CLI 2.81.0 o
posterior (`winget install --id GitHub.cli`; si ya lo tienes, `winget upgrade
--id GitHub.cli`).

### Instalar en Windows

1. Descarga `GitCRAFT_<versión>_x64-setup.exe` (o `_arm64-setup.exe`, si tu
   equipo es ARM) de [Releases](https://github.com/epmoreno/GitCRAFT/releases).
2. Comprueba su SHA-256 con el de las notas de la Release (en PowerShell):

   ```powershell
   Get-FileHash "$HOME\Downloads\GitCRAFT_<versión>_x64-setup.exe"
   ```

3. Ábrelo y sigue los pasos. Se instala solo para tu usuario, sin permisos de
   administrador, en `%LOCALAPPDATA%\GitCRAFT`. Si falta WebView2 (Windows 11 ya
   lo trae), lo descarga de Microsoft.

### El aviso de SmartScreen

**Qué sale.** Al abrir el instalador descargado, una ventana azul de Microsoft
Defender SmartScreen: «Windows protegió su PC», que dice que ha impedido el
inicio de una aplicación desconocida. Antes, el navegador puede avisar de que
el archivo no se descarga habitualmente.

**Cómo abrirlo, sin desactivar nada:**

1. Comprueba antes el SHA-256 (arriba).
2. Si avisa el navegador, conserva el archivo (en Edge, **… → Conservar**).
3. En la ventana azul, pulsa **Más información**. Aparecen el nombre del archivo
   y «Editor desconocido»: es lo esperado, porque no está firmado.
4. Pulsa **Ejecutar de todas formas**.

La excepción es solo para ese archivo: SmartScreen sigue activo para todo lo
demás. **No desactives** SmartScreen, Microsoft Defender ni Smart App Control.

**Si lo bloquea Smart App Control,** no hay botón para seguir: esa protección de
Windows 11 bloquea las aplicaciones sin firma que Microsoft no conoce y no
permite excepciones para una sola. Con ella activada, GitCRAFT puede no
instalarse; no recomendamos desactivarla por ello.

### ¿Por qué no está firmado en Windows?

GitCRAFT es software libre y gratuito (licencia MIT), y un certificado de firma
de código cuesta dinero todos los años. Sin él, Windows no puede decir quién ha
hecho el instalador. Firmarlo tampoco quitaría siempre el aviso: SmartScreen
también tiene en cuenta cuánta gente ha descargado ya ese archivo.
[Microsoft lo explica aquí](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation).
Queda pendiente estudiar la firma gratuita para proyectos de código abierto de
SignPath Foundation.

Puedes comprobar lo que descargas igual que en macOS: SHA-256 y VirusTotal en
las notas de cada Release, o compilarlo tú con `npm run dist` (en Windows, con
[7-Zip](https://www.7-zip.org) instalado, que usa para comprobar el instalador
por dentro).

### Desinstalar en Windows

**Configuración → Aplicaciones → Aplicaciones instaladas → GitCRAFT →
Desinstalar.** Marca **«Eliminar los datos de aplicación»** para borrar también
sus ajustes y recientes; no borra ningún repositorio.

## Linux

> **Todavía no publicado:** el paquete llega con la v0.4.1.

Un `.deb` para Ubuntu 22.04 o posterior, en x64 (`amd64`) y ARM64 (`arm64`). El de
ARM64 solo se prueba automáticamente, en el CI.

### Instalar en Linux

1. Descarga `GitCRAFT_<versión>_amd64.deb` (o `_arm64.deb`) de
   [Releases](https://github.com/epmoreno/GitCRAFT/releases).
2. Comprueba su SHA-256 con el de las notas de la Release:

   ```sh
   sha256sum GitCRAFT_<versión>_amd64.deb
   ```

3. Instálalo con apt (con `./` delante), que añade lo que falte, Git incluido:

   ```sh
   sudo apt install ./GitCRAFT_<versión>_amd64.deb
   ```

El paquete no está firmado ni viene de un repositorio de apt: el SHA-256 es la
comprobación.

`gh` es opcional, para las funciones de GitHub: GitHub CLI 2.81.0 o posterior,
instalado desde [su repositorio oficial](https://github.com/cli/cli/blob/trunk/docs/install_linux.md).
El que ofrece Ubuntu por apt es demasiado antiguo (2.46.0 en Ubuntu 26.04):
GitCRAFT lo detecta y lo explica. Para ver tu versión: `gh --version`.

### Desinstalar en Linux

`sudo apt remove git-craft`. Los ajustes y recientes se quedan en
`~/.config/dev.gitcraft.app`, `~/.local/share/dev.gitcraft.app` y
`~/.cache/dev.gitcraft.app` (los que existan): llévalas a la Papelera si
también quieres quitarlos. Tus repositorios y Git no se tocan.
