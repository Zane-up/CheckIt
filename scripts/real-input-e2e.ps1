# CheckIt real-input E2E v5: MainWindowHandle + simple P/Invoke, DPI-aware.
Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public class DpiFix { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }'
[DpiFix]::SetProcessDPIAware() | Out-Null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class U32 { [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y); [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e); [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; } }'

function Shot($path) {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save($path); $g.Dispose(); $bmp.Dispose()
}
function Click($x, $y) {
    [U32]::SetCursorPos([int]$x, [int]$y) | Out-Null
    Start-Sleep -Milliseconds 150
    [U32]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [U32]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
}

$proc = Get-Process checkit -ErrorAction Stop
$hwnd = $proc.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "NO_WINDOW"; exit 1 }
$rect = New-Object U32+RECT
[U32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$W = $rect.R - $rect.L; $H = $rect.B - $rect.T
Write-Output "widget physical rect: L=$($rect.L) T=$($rect.T) W=$W H=$H"
[U32]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 400

$plusX = [int]($rect.R - 39); $plusY = [int]($rect.T + 39)
Write-Output "[1] click + at $plusX,$plusY"
Click $plusX $plusY
Start-Sleep -Milliseconds 900
Shot "E:\e2e_r1_open.png"

$inX = [int]($rect.L + 240); $inY = [int]($rect.T + 176)
Write-Output "[2] click title input at $inX,$inY"
Click $inX $inY
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("RealInputTask")
Start-Sleep -Milliseconds 300

Write-Output "[3] Enter to save"
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Start-Sleep -Milliseconds 1500
Shot "E:\e2e_r2_saved.png"

$mgrX = [int]($rect.R - 67); $mgrY = [int]($rect.B - 28)
Write-Output "[4] click open-manager at $mgrX,$mgrY"
Click $mgrX $mgrY
Start-Sleep -Milliseconds 2500
Shot "E:\e2e_r3_manager.png"

Write-Output "DONE"
