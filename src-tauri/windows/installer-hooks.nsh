!include WinVer.nsh

Var CNshellDesktopShortcutExisted

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${AtLeastBuild} 19045
    MessageBox MB_ICONSTOP|MB_OK "CNshell requires Windows 10 22H2 (build 19045) or later."
    Abort
  ${EndIf}
  StrCpy $CNshellDesktopShortcutExisted 0
  IfFileExists "$DESKTOP\${PRODUCTNAME}.lnk" 0 +2
    StrCpy $CNshellDesktopShortcutExisted 1
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $CNshellDesktopShortcutExisted != 1
    Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  ${EndIf}
!macroend
