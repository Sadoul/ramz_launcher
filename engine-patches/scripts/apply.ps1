# Apply all engine patches into MCP-Reborn source tree.
# Before overwriting a vanilla file, snapshot it into ../original/ if not already there.

. $PSScriptRoot\_paths.ps1

Assert-Path $MCP_SRC      'MCP-Reborn src'
Assert-Path $RAMZE_PATCHES 'engine-patches/patches'

if (-not (Test-Path $RAMZE_ORIGINAL)) {
    New-Item -ItemType Directory -Force -Path $RAMZE_ORIGINAL | Out-Null
}

$applied = 0
foreach ($name in $RAMZE_FILE_MAP.Keys) {
    $rel       = $RAMZE_FILE_MAP[$name]
    $vanilla   = Join-Path $MCP_SRC $rel
    $patchFile = Get-ChildItem -Path $RAMZE_PATCHES -Recurse -Filter $name -ErrorAction SilentlyContinue | Select-Object -First 1
    $snapshot  = Join-Path $RAMZE_ORIGINAL $name

    if (-not $patchFile) { Write-Warning "patch missing: $name (skipped)"; continue }
    if (-not (Test-Path $vanilla)) { Write-Warning "vanilla missing: $vanilla (skipped)"; continue }

    if (-not (Test-Path $snapshot)) {
        Copy-Item $vanilla $snapshot
        Write-Host "  snapshot $name -> original/" -ForegroundColor DarkGray
    }

    Copy-Item $patchFile.FullName $vanilla -Force
    Write-Host "  applied  $name -> $rel" -ForegroundColor Green
    $applied++
}

Write-Host "`nApplied $applied patch(es)." -ForegroundColor Cyan
