# release.ps1 - локальная сборка и публикация релиза Project Doomsday Launcher

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$conf = Get-Content "src-tauri\tauri.conf.json" | ConvertFrom-Json
$VERSION = $conf.version
$TAG     = "v$VERSION"

# Автоинкремент patch если тег уже существует
while (git tag -l $TAG) {
    Write-Host "[INFO] Тег $TAG уже существует, авто-инкремент версии..." -ForegroundColor Yellow
    $parts = $VERSION -split '\.'
    $parts[-1] = [int]$parts[-1] + 1
    $VERSION = $parts -join '.'
    $TAG = "v$VERSION"

    # Обновляем версию в файлах
    $tauriConf = Get-Content "src-tauri\tauri.conf.json" -Raw
    $tauriConf = $tauriConf -replace '"version": "[^"]*"', "`"version`": `"$VERSION`""
    Set-Content "src-tauri\tauri.conf.json" $tauriConf -NoNewline

    (Get-Content "src-tauri\Cargo.toml") -replace '^version = ".*"', "version = `"$VERSION`"" |
        Set-Content "src-tauri\Cargo.toml"

    (Get-Content "stub-rs\Cargo.toml") -replace '^version = ".*"', "version = `"$VERSION`"" |
        Set-Content "stub-rs\Cargo.toml"

    Write-Host "[INFO] Версия обновлена до $VERSION" -ForegroundColor Cyan
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "  Project Doomsday Launcher - Release $TAG" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "[1/4] Генерация иконок из images/icons/logo_square.png..." -ForegroundColor Green
npx @tauri-apps/cli icon images/icons/logo_square.png
if ($LASTEXITCODE -ne 0) { throw "Ошибка генерации иконок" }

Write-Host ""
Write-Host "[2/4] Сборка Tauri (без NSIS — только portable .exe)..." -ForegroundColor Green
npx tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "Ошибка сборки Tauri" }

$mainExe = "src-tauri\target\release\pd-launcher-core.exe"
if (-not (Test-Path $mainExe)) { throw "Main launcher exe не найден: $mainExe" }
Write-Host "  -> Main: $mainExe" -ForegroundColor DarkGray

Write-Host ""
Write-Host "[3/4] Сборка Project-Doomsday-Launcher.exe (stub)..." -ForegroundColor Green
Push-Location stub-rs
cargo build --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "Ошибка сборки stub" }
Pop-Location

$stubExe = "stub-rs\target\release\Project-Doomsday-Launcher.exe"
if (-not (Test-Path $stubExe)) { throw "Stub exe не найден: $stubExe" }

Write-Host ""
Write-Host "[4/4] Публикация релиза $TAG на GitHub..." -ForegroundColor Green

$staged = git status --porcelain
if ($staged) {
    # Count existing "Коммит N" messages and increment to keep numbering monotone.
    $existing = git log --oneline | Select-String -Pattern '^\S+\s+Коммит\s+\d+\s*$'
    $nextNum = $existing.Count + 1
    $msg = "Коммит $nextNum"
    Write-Host "  -> commit message: $msg" -ForegroundColor DarkGray
    git add -A
    git commit -m $msg
}

git tag $TAG
git push origin main
if ($LASTEXITCODE -ne 0) { throw "Ошибка git push (main)" }
git push origin "refs/tags/$TAG"
if ($LASTEXITCODE -ne 0) { throw "Ошибка git push (tag $TAG)" }

# NSIS-инсталлятор в релиз НЕ загружаем. Только main launcher .exe (его
# скачивает updater для апдейтов и stub для первой установки) + stub.exe.
$releaseFiles = @(
    (Resolve-Path $mainExe).Path,
    (Resolve-Path $stubExe).Path
)
gh release create $TAG `
    --title "Project Doomsday Launcher $TAG" `
    --notes "Обновление лаунчера до версии $TAG" `
    @releaseFiles

if ($LASTEXITCODE -ne 0) { throw "Ошибка создания релиза" }

Write-Host ""
Write-Host "==================================================" -ForegroundColor Green
Write-Host "  Релиз $TAG успешно опубликован!" -ForegroundColor Green
Write-Host "  https://github.com/Sadoul/ramz_launcher/releases/tag/$TAG" -ForegroundColor Green
Write-Host "==================================================" -ForegroundColor Green
