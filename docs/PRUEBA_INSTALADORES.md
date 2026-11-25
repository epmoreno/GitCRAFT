# Prueba de los instaladores de Windows y Linux

Guía para probar a mano los instaladores de la tarea I de `docs/ROADMAP.md`. Lo
que no se prueba aquí no se da por verificado.

| Sistema | Dónde | Archivo |
| --- | --- | --- |
| Windows 11 x64 | en un ordenador, sin máquina virtual | `GitCRAFT_<versión>_x64-setup.exe` |
| Ubuntu 26.04 x64 | en una máquina virtual | `GitCRAFT_<versión>_amd64.deb` |

Los de ARM64 (`…_arm64-setup.exe` y `…_arm64.deb`) solo los prueba el CI.

Anota en cada paso si sale bien o qué ves distinto (con captura si puedes). Si
un paso marcado con ⛔ falla, para y dímelo: lo demás no tendría sentido.

## Reglas

- **Siempre con los repositorios de prueba de esta guía, nunca con repositorios
  reales.** No abras, clones ni arrastres a GitCRAFT ninguno tuyo mientras dure
  la prueba.
- **No cambies tu identidad global ni tu sesión de GitHub:** en Ajustes →
  Identidad de Git y Ajustes → Cuenta de GitHub, solo mira. El repositorio de
  prueba lleva su propia identidad, inventada.
- **No desactives ninguna protección** (SmartScreen, Smart App Control,
  Defender, AppArmor…). Si algo bloquea GitCRAFT sin dar opción a seguir, para
  y dímelo.
- En Ubuntu, haz una **instantánea** de la máquina virtual antes de instalar:
  así la instalación limpia se puede repetir.

## Qué hace falta

- **Windows:** Git para Windows, de <https://git-scm.com/downloads/win> o con
  `winget install --id Git.Git -e` en PowerShell (trae **Git Bash**, con el que
  se crea el repositorio de prueba). Si ya lo tienes, vale: `git --version`
  tiene que dar 2.30 o posterior.
- **Ubuntu:** nada. El `.deb` instala Git si falta; si lo quieres antes, `sudo
  apt install git`.
- **`gh` (GitHub CLI) es opcional** en los dos: sin él, GitCRAFT explica cómo
  instalarlo y el resto funciona.
- No hace falta Rust, Node ni Visual Studio.

## Descargar los instaladores

1. En el navegador, **con la sesión iniciada en GitHub** (sin sesión no deja
   descargar), abre la página de la ejecución que te haya pasado: en el
   repositorio, **Actions → Installers**.
2. Abajo, en **Artifacts**, pulsa el archivo de tu sistema. Se descarga tal
   cual, sin `.zip`. Se borran a los 7 días.
3. Descárgalo en el mismo sistema donde lo vas a probar, para que los avisos
   sean los de verdad.
4. En Windows, el navegador puede avisar de que el archivo «no se descarga
   habitualmente»: en Edge, pasa el ratón por el archivo, **… → Conservar** y,
   si vuelve a preguntar, **Conservar de todos modos**. Apunta qué ha salido.

Comprueba el SHA-256 **antes de abrirlo**. Tiene que ser el que te haya dado y
el que enseña GitHub en la columna **Digest**. ⛔

- **Windows,** en PowerShell:

  ```powershell
  cd "$HOME\Downloads"
  Get-FileHash .\GitCRAFT_<versión>_x64-setup.exe -Algorithm SHA256
  ```

  Sale en mayúsculas: es lo mismo.
- **Ubuntu,** en la terminal, en la carpeta de las descargas:

  ```sh
  sha256sum GitCRAFT_<versión>_amd64.deb
  ```

## A. Windows 11 x64

### A1. Instalar

1. Doble clic en el instalador. Si aparece SmartScreen («Windows protegió su
   PC»), pulsa **Más información**, comprueba que dice el nombre del archivo y
   «Editor desconocido», y **Ejecutar de todas formas**. Apunta si ha salido y
   con qué texto: lo cuentan las guías de instalación.
2. Si lo bloquea **Smart App Control**, no hay botón para seguir: no lo
   desactives y dímelo. ⛔
3. El instalador sale en tu idioma, enseña la licencia MIT, no pide permisos de
   administrador y termina sin errores. ⛔
4. GitCRAFT está en el menú Inicio con su icono, y en **Configuración →
   Aplicaciones → Aplicaciones instaladas** con su versión y el editor
   «GitCRAFT».
5. Ábrelo: se abre la ventana, sin ninguna consola negra detrás. ⛔
6. Solo si el equipo no tenía Git: GitCRAFT lo explica y propone `winget
   install --id Git.Git`. Instálalo, cierra GitCRAFT y ábrelo: ya no falta. Si
   ya lo tenías, **no lo desinstales** para probar esto.

### A2. Usar

Crea el repositorio de prueba (más abajo, «Repositorio de prueba») en **Git
Bash** y pasa la lista C.

Además, solo en Windows:

7. **Papelera:** descarta `notas.txt` → va a la **Papelera de reciclaje** (el
   texto lo dice así). «Deshacer» lo devuelve igual y lo quita de la Papelera.
8. **Consolas:** durante todo el uso no parpadea ninguna ventana de consola.
9. **Rutas:** abre el repositorio de prueba desde la raíz, desde su carpeta
   `sub` y escribiendo la ruta con otras mayúsculas: una sola pestaña y una
   sola línea en «Recientes».
10. **CRLF:** en Git Bash, dentro del repositorio de prueba, `git config
    core.autocrlf true`; edita dos líneas separadas de `lista.txt` con el Bloc
    de notas, prepara solo una línea, haz el commit y descarta la otra. En el
    diff del commit solo sale esa línea.
11. **Escalado:** Configuración → Pantalla → 125 % y 150 %: el grafo y el texto
    se ven nítidos, y ningún diálogo se corta. Vuelve a dejar el que tenías.
12. **GitHub y textos:** Ajustes → Cuenta de GitHub: sin `gh`, propone `winget`;
    con `gh`, dice quién tiene la sesión y dónde la guarda (no la cierres).
    Ajustes → Identidad de Git nombra `%USERPROFILE%\.gitconfig`.

### A3. Reinstalar

13. Con GitCRAFT cerrado, vuelve a ejecutar el mismo instalador y elige
    **«Añadir o reinstalar componentes»**: no duplica la entrada en «Aplicaciones instaladas», y se conservan
    recientes y ajustes.

### A4. Desinstalar y comprobar que no queda nada

14. Cierra GitCRAFT y desinstálalo desde **Aplicaciones instaladas**. En la
    ventana que sale, marca **«Eliminar los datos de aplicación»** (borra
    solo los ajustes y recientes de GitCRAFT; ningún repositorio). ⛔
15. En PowerShell, esto solo mira y no cambia nada. Las siete líneas tienen que
    decir `False`: ⛔

    ```powershell
    Test-Path "$env:LOCALAPPDATA\GitCRAFT"
    Test-Path "$env:LOCALAPPDATA\dev.gitcraft.app"
    Test-Path "$env:APPDATA\dev.gitcraft.app"
    Test-Path "HKCU:\Software\GitCRAFT"
    Test-Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\GitCRAFT"
    Test-Path "$([Environment]::GetFolderPath('Programs'))\GitCRAFT.lnk"
    Test-Path "$([Environment]::GetFolderPath('Desktop'))\GitCRAFT.lnk"
    ```

    Son, por orden: el programa, los datos de la ventana, los ajustes, lo que
    recuerda el instalador, la entrada de «Aplicaciones instaladas» y los
    accesos directos del menú Inicio y del escritorio. Sin marcar la casilla,
    la segunda, la tercera y la cuarta dan `True`: es lo esperado.
16. El repositorio de prueba sigue intacto, y `git --version` sigue
    funcionando. ⛔
17. Al acabar, lleva la carpeta `gitcraft-prueba-instalador` (en
    `C:\Users\<tu usuario>`) a la Papelera.

## B. Ubuntu 26.04 x64

### B1. Instalar

1. En la terminal, en la carpeta de las descargas (con `./` delante):

   ```sh
   sudo apt install ./GitCRAFT_<versión>_amd64.deb
   ```

   Instala también lo que falte, Git incluido, sin errores. ⛔ Una nota final
   sobre «unsandboxed» o el usuario `_apt` es normal: apt no puede leer tu
   carpeta con su usuario propio.
2. `dpkg -s git-craft`: versión correcta y `Maintainer: GitCRAFT`.
3. GitCRAFT sale en las aplicaciones con su icono. Ábrelo: ventana con la
   interfaz completa. ⛔ Si sale en blanco, dímelo con `echo
   $XDG_SESSION_TYPE` y no cambies nada más.

### B2. Usar

Crea el repositorio de prueba en la terminal y pasa la lista C. Además:

4. **Papelera:** descarta `notas.txt` → va a la Papelera de Archivos.
   «Deshacer» lo devuelve igual y lo quita de la Papelera.
5. **Ventana:** mover, redimensionar, maximizar y cerrar funcionan; la barra de
   título es la del sistema.
6. **Fuentes y nitidez:** el texto y el grafo se ven bien, también al 200 % en
   Configuración → Pantallas.
7. **GitHub:** Ajustes → Cuenta de GitHub: sin `gh`, dice cómo instalarlo.

### B3. Reinstalar

8. `sudo apt install --reinstall ./GitCRAFT_<versión>_amd64.deb`: no falla y se
   conservan los ajustes.

### B4. Desinstalar y comprobar que no queda nada

9. Cierra GitCRAFT y `sudo apt remove git-craft`: desaparece de las
   aplicaciones. ⛔
10. Esto solo mira y no cambia nada. `dpkg` dice que no está instalado y los
    demás, que no existe nada: ⛔

    ```sh
    dpkg -s git-craft
    command -v gitcraft || echo "no está"
    ls -d /usr/share/applications/GitCRAFT.desktop /usr/share/doc/git-craft
    ```

11. Los datos de GitCRAFT (ajustes y recientes) se quedan, como con cualquier
    paquete. Mira cuáles hay:

    ```sh
    ls -d ~/.config/dev.gitcraft.app ~/.local/share/dev.gitcraft.app ~/.cache/dev.gitcraft.app
    ```

    Para quitarlos, en Archivos (Ctrl+H enseña lo oculto) lleva a la Papelera
    **solo** esas carpetas `dev.gitcraft.app`. Repite la orden: no queda
    ninguna.
12. El repositorio de prueba y Git siguen. ⛔ Al acabar, lleva la carpeta
    `gitcraft-prueba-instalador` a la Papelera, o vuelve a la instantánea.

Las bibliotecas que apt instaló con GitCRAFT las quitaría `sudo apt autoremove`,
pero esa orden también puede quitar otras que no son suyas: no hace falta.

## Repositorio de prueba

**Windows:** abre **Git Bash** desde el menú Inicio (no PowerShell), copia el
bloque entero, pégalo con Mayús+Insert o el botón derecho → *Paste*, y pulsa
Intro. La carpeta queda en `C:\Users\<tu usuario>\gitcraft-prueba-instalador`.

**Ubuntu:** lo mismo en la terminal (Ctrl+Mayús+V). Necesita Git, así que se
hace después de instalar el `.deb`.

Crea `~/gitcraft-prueba-instalador`; si ya existe, se detiene sin tocar nada.

```sh
mkdir ~/gitcraft-prueba-instalador && cd ~/gitcraft-prueba-instalador && {
git init -q --bare -b main remoto.git
git init -q -b main repo
cd repo
git config user.name "Prueba"
git config user.email "prueba@gitcraft.invalid"
git config commit.gpgsign false
git remote add origin ../remoto.git
mkdir sub
printf 'uno\ndos\ntres\ncuatro\ncinco\nseis\nsiete\nocho\n' > lista.txt
printf 'ejemplo\n' > "sub/año nuevo.txt"
git add . && git commit -q -m "Base"
git branch choca
git branch limpia
printf 'uno\nDOS de main\ntres\ncuatro\ncinco\nseis\nsiete\nocho\n' > lista.txt
git commit -q -am "Cambia la línea dos"
git push -q -u origin main
git switch -q choca
printf 'uno\nDOS de choca\ntres\ncuatro\ncinco\nseis\nsiete\nocho\n' > lista.txt
git commit -q -am "Otra línea dos"
git switch -q limpia
printf 'nuevo\n' > nuevo.txt
git add nuevo.txt && git commit -q -m "Añade nuevo.txt"
git switch -q main
printf 'uno\nDOS de main\ntres\ncuatro\ncinco\nseis\nSIETE\nocho\n' > lista.txt
printf 'sin seguimiento\n' > notas.txt
git log --oneline --graph --all
git status --short
}
```

Debe acabar enseñando cuatro commits, y `lista.txt` modificado y `notas.txt` sin
seguimiento. En Windows pueden salir avisos como «LF will be replaced by CRLF»:
no son errores.

## C. Lo que se prueba en los dos sistemas

1. **Abrir:** «Abrir repositorio…» → la carpeta `repo`. Se ven el grafo con las
   tres ramas, los cambios sin confirmar y `año nuevo.txt` bien escrito. ⛔
2. **Menú y atajos:** los menús Archivo, Edición y Ayuda; Ctrl+T, Ctrl+W,
   Ctrl+, (coma), Ctrl+1, Ctrl+Tab y Ctrl+F. «Acerca de GitCRAFT» dice la
   versión correcta.
3. **Commit:** prepara `lista.txt`, escribe un resumen y Ctrl+Intro. «Deshacer»
   de la barra lo deshace tras enseñar qué cambia; «Rehacer» lo repite. ⛔
4. **Descartar:** con `notas.txt` seleccionado, Ctrl+Supr pide confirmación.
   Sigue con el paso de la Papelera de tu sistema.
5. **Merge limpio:** merge de `limpia` en `main`, con su vista previa. ⛔
6. **Conflicto:** merge de `choca` y «Abortar»: todo queda como antes. Repite el
   merge, elige una versión del bloque, márcalo como resuelto y «Continuar». ⛔
7. **Push y fetch:** Push y después Fetch con el remoto local, sin errores.
8. **Clonar y cancelar:** «Clonar repositorio…» dentro de la carpeta de prueba,
   `gitcraft-prueba-instalador`, con
   `https://github.com/epmoreno/GitCRAFT.git`, termina bien. Después, con
   `https://github.com/git/git.git`, pulsa «Cancelar clonación» a mitad: no
   queda la carpeta. ⛔
9. **Cancelar un fetch:** en el clon de GitCRAFT, en la terminal, `git remote
   add grande https://github.com/git/git.git`; en las opciones de Fetch, «Fetch
   de «grande»», y cancélalo: dice que se ha cancelado y no queda ningún proceso `git`
   (Administrador de tareas o `pgrep -l git`).
10. **Copiar:** «Copiar hash» en el menú de un commit y pegar en el editor de
    texto.
11. **Idioma y texto:** Ajustes → Generales: cambia a inglés y vuelve, y cambia
    el tamaño del texto. Las barras de desplazamiento y los controles son los
    del sistema y se ven bien.
12. **Operaciones:** el panel enseña los comandos de git, sin contraseñas.
13. **Cerrar y volver:** cierra con la ventana y con Salir; al abrir vuelven las
    mismas pestañas.

## Al terminar

Dime, por sistema: qué versión (Windows: `winver`; Ubuntu: `lsb_release -d` y
`echo $XDG_SESSION_TYPE`), el SHA-256 del archivo probado, qué pasos han ido
bien, cuáles no y cuáles no has hecho. Solo se marca como verificado lo que
confirmes.

## Qué repetir con los instaladores nuevos

Después de la primera prueba cambiaron varias cosas (tarea I.7 del ROADMAP). Con
los instaladores nuevos basta con esto, en Windows y en Ubuntu:

1. **Instalar y abrir** encima de lo que haya (o tras desinstalar), y al final
   desinstalar.
2. **Ventana estrecha,** en español y en inglés (Ajustes → General): estrecha la
   ventana hasta el mínimo con el repositorio de prueba abierto. La barra queda
   en una línea; Ajustes sigue a la vista; lo que no cabe está en «…» y
   funciona (Operaciones, Stash, Rama, Publicar). El panel derecho y el cuadro
   de commit se ven enteros, y las etiquetas se leen.
3. **Acerca de GitCRAFT** (menú Ayuda): nombre, versión 0.4.1, licencia, y el
   enlace abre el repositorio en el navegador.
4. **gh:** en Ubuntu, con el gh de apt (2.46.0), Ajustes → Cuenta de GitHub dice
   esa versión, que hace falta la 2.81.0 y cómo instalar la oficial, y
   «Publicar en GitHub» también. En Windows, con un gh reciente, todo como antes.
   No hace falta cerrar ninguna sesión.
5. **Descartar un archivo nuevo que se está viendo:** vuelve el historial, con su
   aviso. Al prepararlo entero, su diff sigue abierto.
6. **Inicializar un repositorio:** propone «main» (en Windows, aunque el
   instalador de Git dejara «master»), o la rama de tu configuración personal.
