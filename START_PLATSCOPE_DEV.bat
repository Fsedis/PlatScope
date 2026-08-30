@echo off
setlocal EnableExtensions

if /I "%~1"=="--run" goto run
if /I "%~1"=="--doctor" goto doctor

rem Keep the console minimized while preserving useful development logs.
start "PlatScope Dev" /min "%ComSpec%" /D /C ""%~f0" --run"
exit /b 0

:doctor
call :check_environment
if errorlevel 1 exit /b 1
echo PlatScope dev environment is ready.
exit /b 0

:run
title PlatScope Dev
call :check_environment
if errorlevel 1 goto failed

cd /d "%PLATSCOPE_APP_DIR%"
if not exist "node_modules\.pnpm" (
  echo Installing frontend dependencies. This is only needed on the first run...
  call pnpm.cmd install --frozen-lockfile
  if errorlevel 1 goto failed
)

echo Starting PlatScope without building the installer...
echo Later runs and code changes will compile incrementally.
echo.
cargo tauri dev
if errorlevel 1 goto failed

exit /b 0

:check_environment
set "PLATSCOPE_ROOT=%~dp0"
set "PLATSCOPE_APP_DIR=%PLATSCOPE_ROOT%apps\desktop"

if not exist "%PLATSCOPE_APP_DIR%\package.json" (
  echo ERROR: application directory was not found: "%PLATSCOPE_APP_DIR%".
  exit /b 1
)

where cargo.exe >nul 2>&1
if errorlevel 1 (
  echo ERROR: Rust/Cargo was not found in PATH.
  exit /b 1
)

where pnpm.cmd >nul 2>&1
if errorlevel 1 (
  echo ERROR: pnpm was not found in PATH.
  exit /b 1
)

where npm.cmd >nul 2>&1
if errorlevel 1 (
  echo ERROR: npm was not found in PATH.
  exit /b 1
)

cargo tauri --version >nul 2>&1
if errorlevel 1 (
  echo ERROR: cargo-tauri is not installed.
  echo Install it with: cargo install tauri-cli --version "^2"
  exit /b 1
)

exit /b 0

:failed
echo.
echo PlatScope did not start. The error is shown above.
echo Press any key to close this window.
pause >nul
exit /b 1
