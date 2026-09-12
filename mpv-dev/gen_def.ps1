<#
.SYNOPSIS
    Builds an MSVC-compatible mpv.lib import library from a libmpv runtime dll.

.DESCRIPTION
    libmpv-sys (the Rust crate amp links against) just emits
    `cargo:rustc-link-lib=mpv`, which on the MSVC toolchain means link.exe
    goes looking for a file literally named mpv.lib. No official libmpv
    Windows build ships one - the shinchiro mpv-dev builds
    (https://sourceforge.net/projects/mpv-player-windows/files/libmpv/) give
    you libmpv-2.dll plus a MinGW-style libmpv.dll.a, neither of which
    link.exe can use directly.

    This script derives mpv.lib from the dll's own export table using
    dumpbin.exe + lib.exe from the VS Build Tools, so you never have to hand
    -write a .def file. Re-run it whenever you update libmpv-2.dll.

.PARAMETER Dll
    Path to the libmpv runtime dll (default: libmpv-2.dll next to this script).

.PARAMETER OutLib
    Path to write the generated import library to (default: mpv.lib next to
    this script, which is where amp-main's build.rs looks for it).

.EXAMPLE
    powershell -File gen_def.ps1
    powershell -File gen_def.ps1 -Dll C:\path\to\libmpv-2.dll
#>
param(
    [string]$Dll = "$PSScriptRoot\libmpv-2.dll",
    [string]$OutLib = "$PSScriptRoot\mpv.lib"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $Dll)) {
    throw "Could not find $Dll - download an mpv-dev-x86_64-*.7z package from " +
          "https://sourceforge.net/projects/mpv-player-windows/files/libmpv/ and extract " +
          "libmpv-2.dll next to this script (or pass -Dll)."
}

function Find-VcTool([string]$Name) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vsInstall = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vsInstall) {
            $found = Get-ChildItem -Path (Join-Path $vsInstall "VC\Tools\MSVC") -Recurse -Filter $Name -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match "HostX64\\x64" } | Select-Object -First 1
            if ($found) { return $found.FullName }
        }
    }
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "Could not locate $Name - run this from a 'Developer PowerShell for VS' prompt, " +
          "or install the 'Desktop development with C++' workload."
}

$dumpbin = Find-VcTool "dumpbin.exe"
$lib = Find-VcTool "lib.exe"
$dllName = Split-Path $Dll -Leaf
$defFile = Join-Path $PSScriptRoot "mpv.def"

Write-Output "Reading exports from $Dll via $dumpbin ..."
$dumpOutput = & $dumpbin /exports $Dll

$names = @()
$inTable = $false
foreach ($line in $dumpOutput) {
    if ($line -match '^\s*ordinal\s+hint\s+RVA\s+name') { $inTable = $true; continue }
    if (-not $inTable) { continue }
    if ($line.Trim() -eq '') { continue }
    if ($line -match '^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)') { $names += $Matches[1] }
}

if ($names.Count -eq 0) {
    throw "dumpbin found no exports in $Dll - is this really the libmpv dll?"
}

"LIBRARY $dllName" | Out-File -Encoding ascii $defFile
"EXPORTS" | Out-File -Encoding ascii -Append $defFile
$names | Out-File -Encoding ascii -Append $defFile
Write-Output "Wrote $($names.Count) exports to $defFile"

Write-Output "Generating $OutLib via $lib ..."
& $lib /def:$defFile /out:$OutLib /machine:x64 /nologo
if ($LASTEXITCODE -ne 0) { throw "lib.exe failed with exit code $LASTEXITCODE" }

Write-Output "Done: $OutLib"
