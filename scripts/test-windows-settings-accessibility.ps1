param(
    [string]$ApplicationPath = "$env:LOCALAPPDATA\CNshell\cnshell.exe",
    [int]$TimeoutSeconds = 30,
    [switch]$WithNarrator
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class CNshellWindowTestNative {
    private const uint KEYEVENTF_KEYUP = 0x0002;

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int command);

    [DllImport("user32.dll")]
    private static extern void keybd_event(byte virtualKey, byte scanCode, uint flags, UIntPtr extraInfo);

    public static void ToggleNarrator() {
        keybd_event(0x5B, 0, 0, UIntPtr.Zero);
        keybd_event(0x11, 0, 0, UIntPtr.Zero);
        keybd_event(0x0D, 0, 0, UIntPtr.Zero);
        keybd_event(0x0D, 0, KEYEVENTF_KEYUP, UIntPtr.Zero);
        keybd_event(0x11, 0, KEYEVENTF_KEYUP, UIntPtr.Zero);
        keybd_event(0x5B, 0, KEYEVENTF_KEYUP, UIntPtr.Zero);
    }
}
"@

function Wait-ForValue {
    param(
        [scriptblock]$Probe,
        [string]$Description
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $value = & $Probe
        if ($null -ne $value) {
            return $value
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "Timed out waiting for $Description"
}

function Find-Element {
    param(
        [System.Windows.Automation.AutomationElement]$Root,
        [string]$Name,
        [System.Windows.Automation.ControlType]$ControlType,
        [switch]$ContainsName
    )

    $conditions = @(
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            $ControlType
        )
    )
    if (-not $ContainsName) {
        $conditions += [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::NameProperty,
            $Name
        )
    }
    $condition = if ($conditions.Count -eq 1) {
        $conditions[0]
    } else {
        [System.Windows.Automation.AndCondition]::new($conditions)
    }
    $matches = $Root.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        $condition
    )
    foreach ($match in $matches) {
        if (-not $ContainsName -or $match.Current.Name.Contains($Name)) {
            return $match
        }
    }
    return $null
}

function Find-RequiredElement {
    param(
        [System.Windows.Automation.AutomationElement]$Root,
        [string]$Name,
        [System.Windows.Automation.ControlType]$ControlType,
        [switch]$ContainsName
    )

    return Wait-ForValue -Description "$ControlType '$Name'" -Probe {
        Find-Element -Root $Root -Name $Name -ControlType $ControlType -ContainsName:$ContainsName
    }
}

function Invoke-Element {
    param([System.Windows.Automation.AutomationElement]$Element)

    $pattern = $Element.GetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern
    )
    $pattern.Invoke()
}

function Set-ElementValue {
    param(
        [System.Windows.Automation.AutomationElement]$Element,
        [string]$Value
    )

    $pattern = $Element.GetCurrentPattern(
        [System.Windows.Automation.ValuePattern]::Pattern
    )
    $pattern.SetValue($Value)
}

function Assert-Contained {
    param(
        [System.Windows.Rect]$Outer,
        [System.Windows.Rect]$Inner,
        [string]$Description
    )

    if ($Inner.Width -le 0 -or $Inner.Height -le 0) {
        throw "$Description has an empty bounding rectangle"
    }
    if (
        $Inner.Left -lt ($Outer.Left - 1) -or
        $Inner.Top -lt ($Outer.Top - 1) -or
        $Inner.Right -gt ($Outer.Right + 1) -or
        $Inner.Bottom -gt ($Outer.Bottom + 1)
    ) {
        throw "$Description is outside the CNshell window at the current DPI"
    }
}

if (-not (Test-Path -LiteralPath $ApplicationPath -PathType Leaf)) {
    throw "CNshell executable was not found: $ApplicationPath"
}

$narrator = $null
$process = $null
try {
    Get-Process cnshell -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    [void](Wait-ForValue -Description "previous CNshell processes to exit" -Probe {
        if ($null -eq (Get-Process cnshell -ErrorAction SilentlyContinue)) { return $true }
        return $null
    })

    $process = Start-Process -FilePath $ApplicationPath -PassThru
    [void]$process.WaitForInputIdle($TimeoutSeconds * 1000)
    $process.Refresh()
    if ($process.MainWindowHandle -eq 0) {
        throw "CNshell did not create a main window"
    }

    [void][CNshellWindowTestNative]::ShowWindow($process.MainWindowHandle, 9)
    [void][CNshellWindowTestNative]::SetForegroundWindow($process.MainWindowHandle)

    $processCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
        $process.Id
    )
    $window = Wait-ForValue -Description "CNshell UI Automation window" -Probe {
        [System.Windows.Automation.AutomationElement]::RootElement.FindFirst(
            [System.Windows.Automation.TreeScope]::Children,
            $processCondition
        )
    }

    $settingsButton = Find-RequiredElement -Root $window -Name "设置" -ControlType ([System.Windows.Automation.ControlType]::Button)
    Invoke-Element $settingsButton

    $closeButton = Find-RequiredElement -Root $window -Name "关闭" -ControlType ([System.Windows.Automation.ControlType]::Button)
    $basicTab = Find-RequiredElement -Root $window -Name "基础设置" -ControlType ([System.Windows.Automation.ControlType]::TabItem)
    $connectionTab = Find-RequiredElement -Root $window -Name "连接与安全" -ControlType ([System.Windows.Automation.ControlType]::TabItem)
    $automationTab = Find-RequiredElement -Root $window -Name "自动化与集成" -ControlType ([System.Windows.Automation.ControlType]::TabItem)
    $teamTab = Find-RequiredElement -Root $window -Name "团队与云端" -ControlType ([System.Windows.Automation.ControlType]::TabItem)
    $supportTab = Find-RequiredElement -Root $window -Name "关于与支持" -ControlType ([System.Windows.Automation.ControlType]::TabItem)
    $searchBox = Find-RequiredElement -Root $window -Name "搜索设置" -ControlType ([System.Windows.Automation.ControlType]::Edit)
    $cancelButton = Find-RequiredElement -Root $window -Name "取消" -ControlType ([System.Windows.Automation.ControlType]::Button)
    $saveButton = Find-RequiredElement -Root $window -Name "保存设置" -ControlType ([System.Windows.Automation.ControlType]::Button)

    $unexpectedMcp = Find-Element -Root $window -Name "MCP 服务" -ControlType ([System.Windows.Automation.ControlType]::Button) -ContainsName
    if ($null -ne $unexpectedMcp) {
        throw "MCP module was mounted before its category was opened"
    }

    Assert-Contained -Outer $window.Current.BoundingRectangle -Inner $cancelButton.Current.BoundingRectangle -Description "Cancel button"
    Assert-Contained -Outer $window.Current.BoundingRectangle -Inner $saveButton.Current.BoundingRectangle -Description "Save button"

    if ($WithNarrator) {
        if ($null -ne (Get-Process Narrator -ErrorAction SilentlyContinue)) {
            [CNshellWindowTestNative]::ToggleNarrator()
            [void](Wait-ForValue -Description "previous Narrator process to exit" -Probe {
                if ($null -eq (Get-Process Narrator -ErrorAction SilentlyContinue)) { return $true }
                return $null
            })
        }
        [CNshellWindowTestNative]::ToggleNarrator()
        $narrator = Wait-ForValue -Description "Narrator process" -Probe {
            Get-Process Narrator -ErrorAction SilentlyContinue | Select-Object -First 1
        }
        Start-Sleep -Seconds 1

        [void][CNshellWindowTestNative]::ShowWindow($process.MainWindowHandle, 9)
        [void][CNshellWindowTestNative]::SetForegroundWindow($process.MainWindowHandle)
        [void](Wait-ForValue -Description "Close button keyboard focus with Narrator running" -Probe {
            try {
                [void][CNshellWindowTestNative]::SetForegroundWindow($process.MainWindowHandle)
                $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
                if (
                    $null -ne $focused -and
                    $focused.Current.ProcessId -eq $process.Id -and
                    $focused.Current.Name -eq "关闭"
                ) {
                    return $true
                }
                $closeButton.SetFocus()
                if ($closeButton.Current.HasKeyboardFocus) { return $true }
            } catch {
                return $null
            }
            return $null
        })
        [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
        [void](Wait-ForValue -Description "active settings tab keyboard focus with Narrator running" -Probe {
            try {
                if ($basicTab.Current.HasKeyboardFocus) { return $true }
                $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
                if (
                    $null -ne $focused -and
                    $focused.Current.ProcessId -eq $process.Id -and
                    $focused.Current.Name -eq "基础设置"
                ) {
                    return $true
                }
            } catch {
                return $null
            }
            return $null
        })
    }

    Set-ElementValue -Element $searchBox -Value "Mosh"
    $searchResult = Find-RequiredElement -Root $window -Name "高级协议与转发" -ControlType ([System.Windows.Automation.ControlType]::Button) -ContainsName
    $searchResultRole = $searchResult.Current.ControlType.ProgrammaticName
    Invoke-Element $searchResult

    $expandedModule = Find-RequiredElement -Root $window -Name "高级协议与转发" -ControlType ([System.Windows.Automation.ControlType]::Button) -ContainsName
    $expandPattern = $expandedModule.GetCurrentPattern(
        [System.Windows.Automation.ExpandCollapsePattern]::Pattern
    )
    if ($expandPattern.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded) {
        throw "Search did not expand the matching module"
    }
    $selectionPattern = $connectionTab.GetCurrentPattern(
        [System.Windows.Automation.SelectionItemPattern]::Pattern
    )
    if (-not $selectionPattern.Current.IsSelected) {
        throw "Search did not select the connection and security tab"
    }

    $cancelButton = Find-RequiredElement -Root $window -Name "取消" -ControlType ([System.Windows.Automation.ControlType]::Button)
    $saveButton = Find-RequiredElement -Root $window -Name "保存设置" -ControlType ([System.Windows.Automation.ControlType]::Button)
    Assert-Contained -Outer $window.Current.BoundingRectangle -Inner $cancelButton.Current.BoundingRectangle -Description "Cancel button after search"
    Assert-Contained -Outer $window.Current.BoundingRectangle -Inner $saveButton.Current.BoundingRectangle -Description "Save button after search"

    $dpi = Get-ItemPropertyValue -Path "HKCU:\Control Panel\Desktop" -Name LogPixels -ErrorAction SilentlyContinue
    [pscustomobject]@{
        Passed = $true
        WindowsBuild = [Environment]::OSVersion.Version.ToString()
        Dpi = $dpi
        ScalePercent = if ($dpi) { [Math]::Round(($dpi / 96) * 100) } else { $null }
        WindowWidth = [Math]::Round($window.Current.BoundingRectangle.Width)
        WindowHeight = [Math]::Round($window.Current.BoundingRectangle.Height)
        Categories = @(
            $basicTab.Current.Name,
            $connectionTab.Current.Name,
            $automationTab.Current.Name,
            $teamTab.Current.Name,
            $supportTab.Current.Name
        )
        SearchResultRole = $searchResultRole
        ExpandedModule = $expandedModule.Current.Name
        Narrator = if ($WithNarrator) { "running during keyboard/UIA validation" } else { "not requested" }
    } | ConvertTo-Json -Depth 4
}
finally {
    if ($null -ne (Get-Process Narrator -ErrorAction SilentlyContinue)) {
        [CNshellWindowTestNative]::ToggleNarrator()
        Start-Sleep -Milliseconds 800
        Get-Process Narrator -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
