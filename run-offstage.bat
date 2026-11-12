@echo off
REM Offstage launcher (docs/offstage/concept.md).
REM   run-offstage.bat         start the release exe (build first if it is missing or the sources are newer)
REM   run-offstage.bat build   force a rebuild, then start
REM Always build with the Tauri CLI. "cargo build" makes an exe that does not embed the UI.
chcp 65001 >nul
title Offstage
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"

set "EXE=%~dp0src-tauri\target\release\offstage.exe"

if not exist "node_modules" (
    echo === Installing packages ^(pnpm install^) ===
    call pnpm install
    if errorlevel 1 goto :fail
)

if /i "%~1"=="build" goto :build
if not exist "%EXE%" goto :build

powershell -NoProfile -Command ^
  "$exe = (Get-Item -LiteralPath $env:EXE).LastWriteTime;" ^
  "$files = @(Get-ChildItem -Recurse -File apps\offstage\ui, apps\offstage\src-tauri\src, crates\offstage-core\src -ErrorAction SilentlyContinue) + @(Get-Item apps\offstage\src-tauri\Cargo.toml, apps\offstage\src-tauri\tauri.conf.json, crates\offstage-core\Cargo.toml);" ^
  "if ($files | Where-Object { $_.LastWriteTime -gt $exe }) { exit 1 } else { exit 0 }"
if errorlevel 1 goto :build
goto :start

:build
tasklist /fi "imagename eq offstage.exe" 2>nul | find /i "offstage.exe" >nul
if not errorlevel 1 (
    echo Offstage is running. It must be closed to rebuild.
    choice /c YN /m "Close it now"
    if errorlevel 2 goto :start
    taskkill /im offstage.exe >nul
    timeout /t 2 /nobreak >nul
)
echo === Building Offstage ===
pushd apps\offstage\src-tauri
call "%~dp0node_modules\.bin\tauri.cmd" build --no-bundle
set "RC=%errorlevel%"
popd
if not "%RC%"=="0" goto :fail

:start
start "" "%EXE%"
goto :eof

:fail
echo.
echo === Failed ===
pause
exit /b 1
