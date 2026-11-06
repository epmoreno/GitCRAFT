# Instalar GitCRAFT en macOS (Apple Silicon)

GitCRAFT se distribuye como `.dmg` para Macs con chip Apple (M1 o posterior).
**Probado en macOS 27.0.1; versiones anteriores no verificadas.** Por ahora **no está firmado con un certificado
de desarrollador de Apple ni notarizado**: solo lleva una firma *ad-hoc*
(local, sin identidad), que en Apple Silicon es necesaria para que macOS
acepte ejecutar la app. Por eso la primera vez que la abras, macOS te pedirá
confirmación. No hace falta desactivar Gatekeeper.

## Requisitos

- Git instalado: el de las Command Line Tools de Xcode
  (`xcode-select --install`) o el de Homebrew (`brew install git`). GitCRAFT
  no incluye su propio Git.
- Para trabajar con remotos privados: un credential helper (en macOS,
  `osxkeychain`, que viene con Git) o una clave SSH en el agente. GitCRAFT
  nunca guarda ni pide contraseñas.
- Para la cuenta de GitHub y «Publicar en GitHub»: GitHub CLI
  (`brew install gh`). GitCRAFT no guarda tokens; la sesión la guarda gh en
  el Llavero.

## Generar el `.dmg`

```sh
npm install        # una vez
npm run dist
```

Hay que ejecutarlo en un Mac con chip Apple: compila para la arquitectura del
equipo (arm64). El resultado queda en
`target/release/bundle/dmg/GitCRAFT_<versión>_aarch64.dmg` (o bajo
`$CARGO_TARGET_DIR` si está definido).

## Instalar

1. Abre el `.dmg` con doble clic.
2. Arrastra **GitCRAFT** a la carpeta **Aplicaciones**.
3. Expulsa el disco «GitCRAFT» (botón ⏏ en la barra lateral de Finder).

## Abrir por primera vez

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

## Desinstalar

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
