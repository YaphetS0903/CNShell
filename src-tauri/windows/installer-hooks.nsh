Var CNshellDesktopShortcutExisted

!macro NSIS_HOOK_PREINSTALL
  StrCpy $CNshellDesktopShortcutExisted 0
  IfFileExists "$DESKTOP\${PRODUCTNAME}.lnk" 0 +2
    StrCpy $CNshellDesktopShortcutExisted 1
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $CNshellDesktopShortcutExisted != 1
    Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  ${EndIf}
!macroend
