# Run this checkout's Argus beside any installed one.
#
# ARGUS_INSTANCE gives this worktree its own pipe/socket and its own slice of
# the config directory, so the dev daemon never touches the installed one's
# state. The client finds its daemon by the same name, so iterating is:
#
#   edit -> .\scripts\dev.ps1 -> try it -> close -> again
#
# The daemon keeps running after the client closes, so the next launch skips
# straight to connecting; a rebuild only pays for what changed. -Stop ends
# this checkout's daemon, found by executable path — an installed Argus
# living elsewhere is not touched.
param(
    [string]$Instance = "dev",
    [switch]$Stop
)

$ErrorActionPreference = "Stop"
$env:ARGUS_INSTANCE = $Instance

if ($Stop) {
    Get-Process argusd -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Path -and $_.Path.StartsWith($PWD.Path, [StringComparison]::OrdinalIgnoreCase)
        } |
        Stop-Process -Force
    return
}

cargo build -q -p argus -p argusd
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& ".\target\debug\argus.exe"
