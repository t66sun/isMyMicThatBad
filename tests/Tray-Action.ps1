param(
    [ValidateSet('Click','Show','Processing','Exit')][string]$Action='Click',
    [Parameter(Mandatory)][ValidateRange(1,2147483647)][int]$TargetProcessId
)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class TrayTarget {
    private delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr parameter);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int GetClassName(IntPtr hwnd, StringBuilder className, int maxCount);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")]
    private static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetMenuString(IntPtr menu, uint position, StringBuilder text, int maxCount, uint flags);
    [DllImport("user32.dll")]
    private static extern uint GetMenuItemID(IntPtr menu, int position);

    public static IntPtr[] FindWindows(int targetProcessId, string targetClassName) {
        var matches = new List<IntPtr>();
        EnumWindows((hwnd, _) => {
            var className = new StringBuilder(256);
            if (GetClassName(hwnd, className, className.Capacity) > 0 &&
                String.Equals(className.ToString(), targetClassName, StringComparison.Ordinal)) {
                GetWindowThreadProcessId(hwnd, out uint processId);
                if (processId == (uint)targetProcessId) matches.Add(hwnd);
            }
            return true;
        }, IntPtr.Zero);
        return matches.ToArray();
    }

    // shortcut: tray-icon 0.25 uses callback 6002; revisit when upgrading tray-icon.
    public static bool PostTrayButton(IntPtr hwnd, uint buttonMessage) =>
        PostMessage(hwnd, 6002, IntPtr.Zero, new IntPtr((int)buttonMessage));

    public static bool ActivateMenu(IntPtr popup, IntPtr owner, string name) {
        var menu = SendMessage(popup, 0x01E1, IntPtr.Zero, IntPtr.Zero);
        for (int i = 0; i < GetMenuItemCount(menu); i++) {
            var text = new StringBuilder(256);
            GetMenuString(menu, (uint)i, text, text.Capacity, 0x400);
            if (text.ToString() != name) continue;
            uint id = GetMenuItemID(menu, i);
            if (id == UInt32.MaxValue) return false;
            PostMessage(owner, 0x001F, IntPtr.Zero, IntPtr.Zero);
            return PostMessage(owner, 0x0111, new IntPtr(id), IntPtr.Zero);
        }
        return false;
    }
}
'@

function Find-TargetWindows([string]$className) {
    @([TrayTarget]::FindWindows($TargetProcessId,$className))
}

$trayWindows=Find-TargetWindows 'tray_icon_app'
if ($trayWindows.Count -ne 1) {
    throw "Expected exactly one tray_icon_app window for PID $TargetProcessId; found $($trayWindows.Count)"
}
$trayWindow=[IntPtr]$trayWindows[0]
$root=[System.Windows.Automation.AutomationElement]::RootElement
$chevron=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Show Hidden Icons'))
if ($chevron) {
    $chevron.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 300
}
$buttonMessages=if ($Action -eq 'Click') { @(0x0201,0x0202) } else { @(0x0204,0x0205) }
foreach ($buttonMessage in $buttonMessages) {
    if (-not [TrayTarget]::PostTrayButton($trayWindow,[uint32]$buttonMessage)) {
        throw "Could not post tray callback $buttonMessage to PID $TargetProcessId"
    }
    Start-Sleep -Milliseconds 50
}
if ($Action -eq 'Click') { return }

$expectedName=switch($Action){'Show'{'显示窗口'} 'Processing'{'开启处理'} 'Exit'{'退出'}}
$deadline=[DateTime]::UtcNow.AddSeconds(5)
$activated=$false
do {
    foreach ($menuHandle in (Find-TargetWindows '#32768')) {
        $activated=[TrayTarget]::ActivateMenu($menuHandle,$trayWindow,$expectedName)
        if ($activated) { break }
    }
    if (-not $activated) { Start-Sleep -Milliseconds 100 }
} while (-not $activated -and [DateTime]::UtcNow -lt $deadline)
if (-not $activated) {
    throw "Tray menu item not found for PID ${TargetProcessId}: $expectedName"
}
