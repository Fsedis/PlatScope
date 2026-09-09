; Отдельный файл с версией в имени обходит старую иконку в кэше Windows.
!define PLATSCOPE_ICON_SOURCE "${__FILEDIR__}\..\icons\icon.ico"
!define MUI_CUSTOMFUNCTION_GUIEND PlatScopeRefreshShortcuts

Var PlatScopeInstalled
Var PlatScopeExecutable
Var PlatScopeIcon
Var PlatScopeStartMenuLink

; В стеке — путь ярлыка. Меняем только иконку и только у нашего приложения:
; аргументы, рабочая папка и AppUserModelID сохраняются через IPersistFile.
Function PlatScopeRefreshShortcut
  Exch $R0
  Push $0
  Push $1
  Push $2
  Push $3
  IfFileExists "$R0" 0 done
  !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ""
  ${If} $0 P<> 0
    ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}",.r1)'
    ${If} $1 P<> 0
      ${IPersistFile::Load} $1 '("$R0", ${STGM_READWRITE}).r3'
      ${If} $3 = 0
        ${IShellLink::GetPath} $0 '(.r2, ${NSIS_MAX_STRLEN}, 0, ${SLGP_RAWPATH}).r3'
        ${If} $3 = 0
        ${AndIf} $2 == $PlatScopeExecutable
          ${IShellLink::SetIconLocation} $0 '("$PlatScopeIcon", 0).r3'
          ${If} $3 = 0
            ${IPersistFile::Save} $1 '("$R0", 1).r3'
            ${If} $3 = 0
              System::Call 'shell32::SHChangeNotify(i 0x2000, i 0x2005, w R0, p 0)'
            ${EndIf}
          ${EndIf}
        ${EndIf}
      ${EndIf}
      ${IUnknown::Release} $1 ""
    ${EndIf}
    ${IUnknown::Release} $0 ""
  ${EndIf}
  done:
  Pop $3
  Pop $2
  Pop $1
  Pop $0
  Pop $R0
FunctionEnd

; Повтор на закрытии интерфейса нужен для ярлыка рабочего стола:
; при обычной установке Tauri создаёт его на последней странице мастера.
Function PlatScopeRefreshShortcuts
  ${If} $PlatScopeInstalled != 1
    Return
  ${EndIf}
  Push $PlatScopeStartMenuLink
  Call PlatScopeRefreshShortcut
  Push "$DESKTOP\PlatScope.lnk"
  Call PlatScopeRefreshShortcut
  Push $R1
  Push $R2
  FindFirst $R1 $R2 "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\*.lnk"
  ${DoWhile} $R2 != ""
    Push "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\$R2"
    Call PlatScopeRefreshShortcut
    FindNext $R1 $R2
  ${Loop}
  FindClose $R1
  Pop $R2
  Pop $R1
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  File /oname=platscope-icon-${VERSION}.ico "${PLATSCOPE_ICON_SOURCE}"
  StrCpy $PlatScopeExecutable "$INSTDIR\${MAINBINARYNAME}.exe"
  StrCpy $PlatScopeIcon "$INSTDIR\platscope-icon-${VERSION}.ico"
  !if "${STARTMENUFOLDER}" != ""
    StrCpy $PlatScopeStartMenuLink "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    StrCpy $PlatScopeStartMenuLink "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
  StrCpy $PlatScopeInstalled 1
  Call PlatScopeRefreshShortcuts
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\platscope-icon-${VERSION}.ico"
  RMDir "$INSTDIR"
!macroend
