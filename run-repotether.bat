@echo off
REM RepoTether launcher.
REM   run-repotether.bat        start the release exe (rebuild first if sources are newer)
REM   run-repotether.bat dev    start with "pnpm tauri dev" (hot reload, keeps this window)
REM   run-repotether.bat build  force a rebuild, then start
chcp 65001 >nul
title RepoTether
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"

set "EXE=%~dp0src-tauri\target\release\repotether.exe"

if not exist "node_modules" (
    echo === Installing packages ^(pnpm install^) ===
    call pnpm install
    if errorlevel 1 goto :fail
)

if /i "%~1"=="dev" (
    echo === RepoTether dev mode ===
    echo Close the app window to stop.
    echo.
    call pnpm tauri dev
    if errorlevel 1 goto :fail
    goto :eof
)

if /i "%~1"=="build" goto :build
if not exist "%EXE%" goto :build

REM Rebuild when any source file is newer than the exe
powershell -NoProfile -Command ^
  "$exe = (Get-Item -LiteralPath $env:EXE).LastWriteTime;" ^
  "$files = @(Get-ChildItem -Recurse -File src, src-tauri\src, static -ErrorAction SilentlyContinue) + @(Get-Item package.json, src-tauri\Cargo.toml, src-tauri\tauri.conf.json, src-tauri\capabilities\default.json);" ^
  "if ($files | Where-Object { $_.LastWriteTime -gt $exe -and $_.Name -ne 'dev-snapshot.json' }) { exit 1 } else { exit 0 }"
if errorlevel 1 goto :build
goto :start

:build
REM A running exe cannot be overwritten, so close it first (with confirmation)
tasklist /fi "imagename eq repotether.exe" 2>nul | find /i "repotether.exe" >nul
if not errorlevel 1 (
    echo RepoTether is running. It must be closed to rebuild.
    choice /c YN /m "Close it now"
    if errorlevel 2 (
        echo Starting the existing build without rebuilding.
        goto :start
    )
    taskkill /im repotether.exe >nul
    timeout /t 2 /nobreak >nul
)
echo === Building RepoTether ^(about 15 seconds; a few minutes the first time^) ===
call pnpm tauri build --no-bundle
if errorlevel 1 goto :fail

:start
start "" "%EXE%"
goto :eof

:fail
echo.
echo === Failed with code %errorlevel% ===
pause
exit /b 1
