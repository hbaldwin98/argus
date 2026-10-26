# Name the lines of a Windows crash log.
#
# A release's executables ship without their .pdb files, so a panic's
# backtrace in the log is every frame `<unknown>`; the hook also writes an
# `offsets in NAME.exe: ...` line, the same on every machine. This reads
# that line against the release's own build: its archive and its symbols
# archive, downloaded with `gh`, or a folder holding the .exe and .pdb.
#
#   .\scripts\symbolize.ps1 crash.log -Version 0.43.0
#   .\scripts\symbolize.ps1 crash.log -Build target\release
#
# Needs llvm-symbolizer, from LLVM (`winget install LLVM.LLVM`).
param(
    [Parameter(Mandatory = $true)][string]$Log,
    [string]$Version,
    [string]$Build,
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

$symbolizer = (Get-Command llvm-symbolizer -ErrorAction SilentlyContinue).Source
if (-not $symbolizer) {
    $symbolizer = Join-Path $env:ProgramFiles "LLVM\bin\llvm-symbolizer.exe"
}
if (-not (Test-Path $symbolizer)) {
    throw "llvm-symbolizer is not installed: winget install LLVM.LLVM"
}

$line = Get-Content $Log | Where-Object { $_ -match '^offsets in (\S+): (.*)$' } | Select-Object -Last 1
if (-not $line) {
    throw "$Log has no 'offsets in' line: it is from a release before 0.43, or no panic was logged"
}
$null = $line -match '^offsets in (\S+): (.*)$'
$exe = $Matches[1]
$offsets = $Matches[2] -split ' '

if (-not $Build) {
    if (-not $Version) {
        throw "say which release wrote the log with -Version, or a local build with -Build"
    }
    $package = "argus-$Version-$Target"
    $Build = Join-Path ([IO.Path]::GetTempPath()) "argus-symbols-$Version"
    if (-not (Test-Path (Join-Path $Build $exe))) {
        $download = Join-Path $Build "download"
        New-Item -ItemType Directory -Force $download | Out-Null
        gh release download "v$Version" --repo hbaldwin98/argus --dir $download --clobber `
            --pattern "$package.zip" --pattern "$package-symbols.zip"
        if ($LASTEXITCODE -ne 0) { throw "could not download v$Version's archives" }
        Expand-Archive (Join-Path $download "$package.zip") $download -Force
        Copy-Item (Join-Path $download "$package\*.exe") $Build
        Expand-Archive (Join-Path $download "$package-symbols.zip") $Build -Force
    }
}

$binary = Join-Path $Build $exe
if (-not (Test-Path $binary)) {
    throw "$binary is not there"
}

# Every frame but the hook's own is a return address, one past its call: a
# byte back is inside the call, so the line named is the call's.
$frame = 0
foreach ($offset in $offsets) {
    if ($offset -eq "-") {
        "${frame}: (another module)"
    } else {
        $at = [Convert]::ToUInt64($offset.Substring(2), 16)
        if ($frame -gt 0) { $at -= 1 }
        $said = & $symbolizer --obj=$binary --relative-address --pretty-print ("0x{0:x}" -f $at)
        "${frame}: " + (($said | Where-Object { $_ }) -join "`n    ")
    }
    $frame += 1
}
