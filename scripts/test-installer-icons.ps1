param(
    [string]$MakeNsis = "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe"
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$testDir = Join-Path $repo "target\installer-icon-test-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $testDir -Force | Out-Null
$hook = Join-Path $repo 'apps\desktop\src-tauri\installer\hooks.nsh'
$exe = Join-Path $testDir 'platscope.exe'
$icon = Join-Path $testDir 'platscope-icon-test.ico'
Copy-Item -LiteralPath "$env:WINDIR\System32\whoami.exe" -Destination $exe
Copy-Item -LiteralPath (Join-Path $repo 'apps\desktop\src-tauri\icons\icon.ico') -Destination $icon
$shell = New-Object -ComObject WScript.Shell
foreach ($name in @('app', 'foreign')) {
    $link = $shell.CreateShortcut((Join-Path $testDir "$name.lnk"))
    $link.TargetPath = if ($name -eq 'app') { $exe } else { "$env:WINDIR\System32\cmd.exe" }
    $link.Arguments = '--keep-this-argument'
    $link.WorkingDirectory = $testDir
    $link.Description = 'Проверка сохранения свойств'
    $link.IconLocation = "$env:WINDIR\System32\shell32.dll,3"
    $link.Save()
}
$foreignHash = (Get-FileHash -LiteralPath (Join-Path $testDir 'foreign.lnk')).Hash
# Тест вызывает только функцию для переданных временных путей;
# реальные ярлыки, окна, установка приложения и закрепления не затрагиваются.
$harness = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
!include LogicLib.nsh
!include "Win\COM.nsh"
!define SLGP_RAWPATH 0x4
!include "@HOOK@"
OutFile "@DIR@\test.exe"
Section
  StrCpy $PlatScopeExecutable "@DIR@\platscope.exe"
  StrCpy $PlatScopeIcon "@DIR@\platscope-icon-test.ico"
  Push "@DIR@\app.lnk"
  Call PlatScopeRefreshShortcut
  Push "@DIR@\foreign.lnk"
  Call PlatScopeRefreshShortcut
  Push "@DIR@\missing.lnk"
  Call PlatScopeRefreshShortcut
SectionEnd
'@
$harness = $harness.Replace('@HOOK@', $hook).Replace('@DIR@', $testDir)
# Отдельно компилируем интеграцию с MUI и оба хука, но не запускаем её.
$integration = @'
Unicode true
RequestExecutionLevel user
!include MUI2.nsh
!include LogicLib.nsh
!include "Win\COM.nsh"
!define SLGP_RAWPATH 0x4
!include "@HOOK@"
!define VERSION "test"
!define MAINBINARYNAME "platscope"
!define PRODUCTNAME "PlatScope"
!define STARTMENUFOLDER ""
OutFile "@DIR@\integration.exe"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "Russian"
Section
  SetOutPath "$INSTDIR"
  !insertmacro NSIS_HOOK_POSTINSTALL
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd
Section "Uninstall"
  !insertmacro NSIS_HOOK_POSTUNINSTALL
SectionEnd
'@
$integrationPath = Join-Path $testDir 'integration.nsi'
[IO.File]::WriteAllText($integrationPath, $integration.Replace('@HOOK@', $hook).Replace('@DIR@', $testDir), [Text.UTF8Encoding]::new($false))
& $MakeNsis /V2 $integrationPath
if ($LASTEXITCODE -ne 0) { throw 'Ошибка компиляции интеграции хуков с MUI' }
$nsi = Join-Path $testDir 'test.nsi'
[IO.File]::WriteAllText($nsi, $harness, [Text.UTF8Encoding]::new($false))
& $MakeNsis /V2 $nsi
if ($LASTEXITCODE -ne 0) { throw 'Ошибка компиляции тестового установщика' }
for ($attempt = 0; $attempt -lt 2; $attempt++) {
    $process = Start-Process -FilePath (Join-Path $testDir 'test.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Ошибка запуска тестового установщика' }
    $link = $shell.CreateShortcut((Join-Path $testDir 'app.lnk'))
    if ($link.IconLocation -ne "$icon,0") { throw "Иконка не обновилась: $($link.IconLocation)" }
    if ($link.TargetPath -ne $exe -or $link.Arguments -ne '--keep-this-argument' -or $link.WorkingDirectory -ne $testDir -or $link.Description -ne 'Проверка сохранения свойств') {
        throw 'Изменились другие свойства ярлыка'
    }
    if ((Get-FileHash -LiteralPath (Join-Path $testDir 'foreign.lnk')).Hash -ne $foreignHash) { throw 'Изменился чужой ярлык' }
    if (Test-Path -LiteralPath (Join-Path $testDir 'missing.lnk')) { throw 'Создан отсутствовавший ярлык' }
}
Write-Output 'Проверки иконок установщика пройдены: замена, сохранение свойств, чужой и отсутствующий ярлыки, повторный запуск.'
