# Prueba de los instaladores de Windows y Linux

Guía para probar a mano, en máquinas virtuales, los instaladores de la tarea I
de `docs/ROADMAP.md`. Lo que no se prueba aquí no se da por verificado.

Anota en cada paso si sale bien o qué ves distinto (con captura si puedes). Si
un paso marcado con ⛔ falla, para y dímelo: lo demás no tendría sentido.

## Antes de empezar

En el Mac hacen falta:

| Qué | De dónde | Espacio |
| --- | --- | --- |
| UTM 5 (gratis y de código abierto; macOS 13 o posterior) | <https://mac.getutm.app> o <https://github.com/utmapp/UTM/releases> | descarga de 290 MB |
| ISO de Windows 11 para Arm64 | <https://www.microsoft.com/software-download/windows11arm64> (el enlace caduca a las 24 h; la página da el SHA-256) | unos 5–8 GB (estimado) |
| ISO de Ubuntu Desktop 26.04.1 LTS para ARM64 | <https://cdimage.ubuntu.com/releases/26.04/release/>, `ubuntu-26.04.1-desktop-arm64.iso` (con `SHA256SUMS`) | 4,2 GB |
| Disco de la máquina de Windows (64 GB virtuales) | — | unos 25–35 GB reales (estimado) |
| Disco de la máquina de Ubuntu (30 GB virtuales) | — | unos 12–15 GB reales (estimado) |

- **Espacio:** unos 60 GB con todo a la vez. Conviene tener 80 GB libres, o
  hacer primero Ubuntu y borrar su ISO y su máquina antes de Windows.
- **Memoria:** una sola máquina encendida a la vez. Con 16 GB en el Mac: 6 GB
  para Windows y 4 GB para Ubuntu.
- **Licencia:** Windows exige una licencia válida de Microsoft también en una
  máquina virtual.
- No hace falta instalar Rust, Node ni Visual Studio en las máquinas: los
  instaladores llegan hechos del CI.

Crear las máquinas en UTM (botón **+** → **Virtualize**):

- **Windows:** elige *Windows*, marca *Install Windows 10 or higher* e *Install
  drivers and SPICE tools*, y selecciona la ISO. 6 GB de memoria, 4 núcleos y
  64 GB de disco. Guía: <https://docs.getutm.app/guides/windows/>. Si la
  pantalla se queda negra al instalar, la guía de UTM lo describe como un
  problema conocido de los controladores gráficos (desde Windows 11 24H2):
  expulsa la ISO de las herramientas, reinicia la máquina, instala Windows y
  monta después las herramientas.
- **Ubuntu:** elige *Linux* y la ISO de escritorio. 4 GB de memoria, 4 núcleos y
  30 GB de disco. La guía de UTM usa Ubuntu Server y añade el escritorio
  (<https://docs.getutm.app/guides/ubuntu/>): es la alternativa si la ISO de
  escritorio no arranca.

Al terminar de instalar cada sistema, haz una **instantánea o un duplicado** de
la máquina en UTM: así la instalación limpia se puede repetir.

Los instaladores se descargan en el Mac y se pasan con la carpeta compartida
de UTM. Archivos que hacen falta:

| Sistema | Archivo |
| --- | --- |
| Windows ARM64 | `GitCRAFT_<versión>_arm64-setup.exe` |
| Windows x64 (en la misma máquina, emulado) | `GitCRAFT_<versión>_x64-setup.exe` |
| Ubuntu ARM64 | `GitCRAFT_<versión>_arm64.deb` |

## A. Windows 11

### A1. Instalar

1. Abre PowerShell en la carpeta del instalador y comprueba el SHA-256 con el
   que te haya dado: `Get-FileHash .\GitCRAFT_<versión>_arm64-setup.exe`. ⛔
2. **Sin Git todavía:** doble clic en el instalador. Si aparece SmartScreen
   («Windows protegió su PC»), pulsa **Más información → Ejecutar de todas
   formas**. No desactives SmartScreen ni Defender. Con la carpeta compartida
   puede no aparecer; apunta si ha salido.
3. El instalador sale en tu idioma, no pide permisos de administrador y
   termina sin errores. ⛔
4. GitCRAFT está en el menú Inicio con su icono, y en **Ajustes → Aplicaciones
   → Aplicaciones instaladas** con su versión y el editor «GitCRAFT», sin
   ningún nombre personal.
5. Ábrelo: se abre la ventana, sin ninguna consola negra detrás. Como falta
   Git, lo explica y propone `winget install --id Git.Git`. ⛔
6. Instala Git en la máquina virtual: `winget install --id Git.Git -e`. Cierra
   GitCRAFT y ábrelo de nuevo: ya no falta Git.

### A2. Usar

Crea el repositorio de prueba (más abajo, «Repositorio de prueba») en **Git
Bash**, y pasa la lista C.

Además, solo en Windows:

7. **Papelera:** descarta `notas.txt` → va a la **Papelera de reciclaje** (el
   texto lo dice así). «Deshacer» lo devuelve igual y lo quita de la Papelera.
8. **Consolas:** durante todo el uso no parpadea ninguna ventana de consola.
9. **Rutas:** abre el repositorio desde la raíz, desde su carpeta `sub` y
   escribiendo la ruta con otras mayúsculas: una sola pestaña y una sola línea
   en «Recientes».
10. **CRLF:** en Git Bash, dentro del repositorio, `git config core.autocrlf
    true`; edita dos líneas separadas de `lista.txt` con el Bloc de notas,
    prepara solo una línea, haz el commit y descarta la otra. En el diff del
    commit solo sale esa línea.
11. **Escalado:** Configuración → Pantalla → 125 % y 150 %: el grafo y el texto
    se ven nítidos, y ningún diálogo se corta.
12. **GitHub y textos:** Ajustes → Cuenta de GitHub: sin `gh`, propone `winget`.
    Ajustes → Identidad de Git nombra `%USERPROFILE%\.gitconfig`. Opcional, si
    instalas `gh`: iniciar sesión abre el navegador, y la pantalla dice dónde
    guarda `gh` la sesión.

### A3. Actualizar y desinstalar

13. Con GitCRAFT instalado, vuelve a ejecutar el mismo instalador: actualiza o
    reinstala sin duplicar la entrada, y se conservan recientes y ajustes.
14. Desinstala desde **Aplicaciones instaladas**: desaparece del menú Inicio y
    la carpeta `%LOCALAPPDATA%\GitCRAFT` ya no está. El repositorio de prueba
    sigue intacto. ⛔
15. Repite A1 (pasos 1, 3 y 5) y una prueba corta con el instalador **x64**:
    Windows lo ejecuta emulado.

## B. Ubuntu

### B1. Instalar

1. En la terminal, en la carpeta del archivo: `sha256sum
   GitCRAFT_<versión>_arm64.deb`, y compáralo. ⛔
2. `sudo apt install ./GitCRAFT_<versión>_arm64.deb` (con `./` delante): instala
   también lo que falte, Git incluido, sin errores. ⛔
3. `dpkg -s git-craft` o el nombre que haya mostrado apt: versión correcta y
   *Maintainer* sin ningún nombre ni correo personal.
4. GitCRAFT sale en las aplicaciones con su icono. Ábrelo: ventana con la
   interfaz completa. ⛔ Si sale en blanco, dímelo con `echo
   $XDG_SESSION_TYPE` y no cambies nada más.

### B2. Usar

Crea el repositorio de prueba en la terminal y pasa la lista C. Además:

5. **Papelera:** descarta `notas.txt` → va a la Papelera de Archivos.
   «Deshacer» lo devuelve igual y lo quita de la Papelera.
6. **Ventana:** mover, redimensionar, maximizar y cerrar funcionan; la barra de
   título es la del sistema.
7. **Fuentes y nitidez:** el texto y el grafo se ven bien, también al 200 % en
   Configuración → Pantallas.
8. **GitHub:** Ajustes → Cuenta de GitHub: sin `gh`, dice cómo instalarlo.
   Opcional, si lo instalas: iniciar sesión abre el navegador, y la pantalla
   dice dónde guarda `gh` la sesión.

### B3. Actualizar y desinstalar

9. Instala otra vez el mismo `.deb`: no falla y se conservan los ajustes.
10. `sudo apt remove` con el nombre del paquete: desaparece de las aplicaciones.
    Git y el repositorio de prueba siguen. ⛔

## Repositorio de prueba

En Git Bash (Windows) o en la terminal (Ubuntu). Crea
`~/gitcraft-prueba-instalador`; si ya existe, se detiene sin tocar nada.

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
seguimiento.

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
8. **Clonar y cancelar:** «Clonar repositorio…» con
   `https://github.com/epmoreno/GitCRAFT.git` termina bien. Después, con
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
`echo $XDG_SESSION_TYPE`), qué pasos han ido bien, cuáles no y cuáles no has
hecho. Solo se marca como verificado lo que confirmes.
