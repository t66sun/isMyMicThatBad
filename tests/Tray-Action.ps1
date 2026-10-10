param([ValidateSet('Click','Show','Processing','Exit')][string]$Action='Click')
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System.Runtime.InteropServices;
public static class TrayMouse {
 [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, System.UIntPtr extra);
}
'@
$root=[System.Windows.Automation.AutomationElement]::RootElement
function Find-Name([string]$name) {
    $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$name)
    if($name -eq 'isMyMicThatBad') {
        $condition=[System.Windows.Automation.AndCondition]::new($condition,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'NotifyItemIcon'))
    }
    $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
}
function Click-Element($element,[bool]$right=$false) {
    if (-not $element) { throw 'Tray control not found' }
    if (-not $right) { $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); return }
    $element.SetFocus()
    Start-Sleep -Milliseconds 100
    # The context-menu key opens the same native menu without DPI-dependent pointer coordinates.
    [TrayMouse]::keybd_event(93,0,0,[UIntPtr]::Zero)
    [TrayMouse]::keybd_event(93,0,2,[UIntPtr]::Zero)
}
$icon=Find-Name 'isMyMicThatBad'
if (-not $icon -or $icon.Current.IsOffscreen) {
    Click-Element (Find-Name 'Show Hidden Icons')
    Start-Sleep -Milliseconds 300
    $icon=Find-Name 'isMyMicThatBad'
}
Click-Element $icon ($Action -ne 'Click')
if ($Action -ne 'Click') {
    Start-Sleep -Milliseconds 250
    $name=switch($Action){'Show'{'显示窗口'} 'Processing'{'开启处理'} 'Exit'{'退出'}}
    $item=Find-Name $name
    if(-not $item) {
        $menu=$root.FindFirst([System.Windows.Automation.TreeScope]::Children,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ClassNameProperty,'#32768'))
        if($menu){$menu.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {Write-Output "$($_.Current.Name) | $($_.Current.ControlType.ProgrammaticName)"}}
        throw "Tray menu item not found: $name"
    }
    Click-Element $item
}
