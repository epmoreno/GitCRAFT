# CI only: installs the NSIS installer in silence, starts the installed app
# with --smoke, uninstalls it and checks that the program is gone.
param(
  [Parameter(Mandatory)] [string] $Installer,
  [Parameter(Mandatory)] [string] $Version,
  [Parameter(Mandatory)] [string] $Arch
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($env:GITHUB_ACTIONS -ne "true") {
  throw "this installs and removes GitCRAFT: it only runs in the CI"
}

function Fail([string] $Text) { throw "installer: $Text" }
function Say([string] $Text) { Write-Output "installer: $Text" }

# The bytes of a file as text, to look for what it carries.
function Get-FileText([string] $Path) {
  [Text.Encoding]::Latin1.GetString([IO.File]::ReadAllBytes($Path))
}

# Runs a program without a console, waits for it and gives its exit code.
function Invoke-Program([string] $Path, [string[]] $Arguments, [int] $Seconds) {
  $out = Join-Path $env:RUNNER_TEMP "out.txt"
  $err = Join-Path $env:RUNNER_TEMP "err.txt"
  $process = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru `
    -RedirectStandardOutput $out -RedirectStandardError $err
  $null = $process.Handle # kept open, or the exit code is lost
  if (-not $process.WaitForExit($Seconds * 1000)) {
    $process.Kill($true)
    Fail "$(Split-Path -Leaf $Path) did not end in $Seconds s"
  }
  $process.WaitForExit()
  Get-Content $out, $err | Out-Host
  return $process.ExitCode
}

# Waits until a condition holds: the uninstaller goes on by itself.
function Wait-Until([scriptblock] $Condition, [string] $Text) {
  for ($i = 0; $i -lt 60; $i++) {
    if (& $Condition) { return }
    Start-Sleep -Seconds 1
  }
  Fail $Text
}

$Installer = (Resolve-Path $Installer).Path
$folder = Join-Path $env:LOCALAPPDATA "GitCRAFT"
$exe = Join-Path $folder "gitcraft.exe"
$uninstaller = Join-Path $folder "uninstall.exe"
$entry = "Microsoft\Windows\CurrentVersion\Uninstall\GitCRAFT"
$userEntry = "HKCU:\Software\$entry"
$machineEntries = "HKLM:\Software\$entry", "HKLM:\Software\WOW6432Node\$entry"
$shortcuts = @(
  (Join-Path ([Environment]::GetFolderPath("Programs")) "GitCRAFT.lnk"),
  (Join-Path ([Environment]::GetFolderPath("Desktop")) "GitCRAFT.lnk")
)

Say "$((Get-CimInstance Win32_OperatingSystem).Caption), $env:PROCESSOR_ARCHITECTURE"

# The installer.
if ((Get-FileText $Installer) -notmatch 'requestedExecutionLevel\s+level="(asInvoker|user)"') {
  Fail "it may ask for administrator rights"
}
$signature = (Get-AuthenticodeSignature $Installer).Status
if ($signature -ne "NotSigned") { Fail "its signature is $signature" }
(Get-Item $Installer).VersionInfo | Format-List ProductName, ProductVersion, FileVersion, CompanyName, LegalCopyright | Out-Host

if (Test-Path $folder) { Fail "already installed" }
$code = Invoke-Program $Installer @("/S") 300
if ($code -ne 0) { Fail "the installer ended with $code" }

# What it installed.
Get-ChildItem -Recurse $folder | Format-Table Length, FullName | Out-Host
if (-not (Test-Path $exe)) { Fail "no gitcraft.exe" }
if (-not (Test-Path $uninstaller)) { Fail "no uninstall.exe" }
foreach ($shortcut in $shortcuts) {
  if (-not (Test-Path $shortcut)) { Fail "no $(Split-Path -Leaf $shortcut)" }
}
$registered = Get-ItemProperty $userEntry
$registered | Format-List DisplayName, DisplayVersion, Publisher, URLInfoAbout | Out-Host
if ($registered.DisplayName -ne "GitCRAFT") { Fail "the name in the list of apps" }
if ($registered.DisplayVersion -ne $Version) { Fail "the version in the list of apps" }
if ($registered.Publisher -ne "GitCRAFT") { Fail "the publisher in the list of apps" }
foreach ($path in $machineEntries) {
  if (Test-Path $path) { Fail "installed for every user" }
}

$info = (Get-Item $exe).VersionInfo
$info | Format-List ProductName, ProductVersion, FileVersion, CompanyName, LegalCopyright | Out-Host
if ($info.ProductVersion -ne $Version) { Fail "the version of gitcraft.exe" }
$bytes = [IO.File]::ReadAllBytes($exe)
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$expected = @{ x64 = 0x8664; arm64 = 0xAA64 }[$Arch]
if ($machine -ne $expected) { Fail ("built for {0:x}" -f $machine) }
$text = Get-FileText $exe
if (-not $text.Contains("Microsoft.Windows.Common-Controls")) {
  Fail "gitcraft.exe has no Common Controls v6 manifest"
}
if ($text -match "VCRUNTIME140") {
  Fail "gitcraft.exe needs the Visual C++ runtime, which Windows may not have"
}
node scripts/dist.mjs --check-folder $folder
if ($LASTEXITCODE -ne 0) { Fail "personal data in the installed folder" }
Say "installed"

$code = Invoke-Program $exe @("--smoke") 300
if ($code -ne 0) { Fail "the installed app ended with $code" }
Say "the installed app started and closed (0)"

# Again over itself, as an update.
$code = Invoke-Program $Installer @("/S") 300
if ($code -ne 0) { Fail "the installer, over itself, ended with $code" }
if (-not (Test-Path $exe)) { Fail "no gitcraft.exe after installing again" }
Say "installed again over itself"

$code = Invoke-Program $uninstaller @("/S") 300
if ($code -ne 0) { Fail "the uninstaller ended with $code" }
Wait-Until { -not (Test-Path $folder) } "the program folder is still there"
Wait-Until { -not (Test-Path $userEntry) } "still in the list of apps"
foreach ($shortcut in $shortcuts) {
  Wait-Until { -not (Test-Path $shortcut) } "$shortcut is still there"
}
Say "removed; the program, its shortcuts and its entry are gone"

# What stays on purpose: the user's data, unless the uninstaller's box is
# ticked, which a silent run does not do.
foreach ($path in (Join-Path $env:APPDATA "dev.gitcraft.app"),
  (Join-Path $env:LOCALAPPDATA "dev.gitcraft.app"),
  "HKCU:\Software\GitCRAFT") {
  Say "user data: $(Test-Path $path) $($path -replace [regex]::Escape($env:USERPROFILE), '%USERPROFILE%')"
}
