# Run from an interactive Windows desktop. Samples the real application from a separate process.
[CmdletBinding()]
param(
    [string]$Exe = "$PSScriptRoot\..\target\release\stand-awhile.exe",
    [string]$Output = "$env:TEMP\stand-awhile-resources.csv",
    [ValidateRange(1, 1000)][int]$Cycles = 30,
    [ValidateRange(100, 60000)][int]$VisibleMs = 15000,
    [ValidateRange(100, 60000)][int]$SettleMs = 2000
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PetMeasurementNative {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindow(string cls, IntPtr title);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flag);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, UIntPtr wp, IntPtr lp,
        uint flags, uint timeout, out UIntPtr result);
}
'@

function Send-WindowMessage([IntPtr]$Window, [uint32]$Message, [uint32]$Parameter) {
    $result = [UIntPtr]::Zero
    if ([PetMeasurementNative]::SendMessageTimeout($Window, $Message, [UIntPtr]$Parameter,
        [IntPtr]::Zero, 2, 10000, [ref]$result) -eq [IntPtr]::Zero) {
        throw "Window message failed or timed out: $Message (error $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
    }
}
function Find-OwnedWindow([string]$Class) {
    $window = [PetMeasurementNative]::FindWindow($Class, [IntPtr]::Zero)
    $ownerId = [uint32]0
    if ($window -ne [IntPtr]::Zero) {
        [void][PetMeasurementNative]::GetWindowThreadProcessId($window, [ref]$ownerId)
    }
    if ($ownerId -eq $app.Id) { return $window }
    return [IntPtr]::Zero
}
function Write-Sample([string]$Phase, [int]$Cycle, [IntPtr]$Window, [double]$TriggerMs = 0) {
    $app.Refresh()
    if ($app.HasExited) { throw 'Application exited before sampling.' }
    $pet = Find-OwnedWindow 'YHB-StandAwhilePetWindow'
    $sample = [pscustomobject]@{
        utc = [DateTime]::UtcNow.ToString('o'); phase = $Phase; cycle = $Cycle
        pid = $app.Id; executable = $exePath; character = $character; period_seconds = $period
        dpi = [PetMeasurementNative]::GetDpiForWindow($Window)
        working_set_bytes = $app.WorkingSet64; private_bytes = $app.PrivateMemorySize64
        gdi_objects = [PetMeasurementNative]::GetGuiResources($app.Handle, 0)
        user_objects = [PetMeasurementNative]::GetGuiResources($app.Handle, 1)
        handles = $app.HandleCount; pet_window_exists = ($pet -ne [IntPtr]::Zero)
        visible_ms = $VisibleMs; settle_ms = $SettleMs; trigger_ms = $TriggerMs
    }
    $sample | Export-Csv -LiteralPath $Output -NoTypeInformation -Encoding UTF8 -Append
    Write-Host "$Phase cycle=$Cycle private=$($sample.private_bytes) GDI=$($sample.gdi_objects) USER=$($sample.user_objects)"
}

$exePath = (Resolve-Path -LiteralPath $Exe).Path
$existing = [PetMeasurementNative]::FindWindow('YHB-StandAwhileWindowClass', [IntPtr]::Zero)
if ($existing -ne [IntPtr]::Zero) {
    $existingId = [uint32]0
    [void][PetMeasurementNative]::GetWindowThreadProcessId($existing, [ref]$existingId)
    throw "Exit the existing Stand Awhile instance (PID $existingId) before measurement."
}
if (Test-Path -LiteralPath $Output) { throw "Output already exists: $Output" }
$configPath = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'yinhaibo\stand-awhile\config.json'
$config = if (Test-Path -LiteralPath $configPath) { Get-Content -LiteralPath $configPath -Raw -Encoding UTF8 | ConvertFrom-Json } else { $null }
$period = if ($null -ne $config.period) { [uint32]$config.period } else { 1200 }
$character = if ($config.character -in @('cat', 'dog')) { $config.character } else { 'cat' }
$app = Start-Process -FilePath $exePath -PassThru -WindowStyle Hidden
Write-Host "Started PID $($app.Id): $exePath"
$main = [IntPtr]::Zero
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $app.Refresh()
        if ($app.HasExited) { throw 'Application exited during startup (possibly another instance).' }
        $main = Find-OwnedWindow 'YHB-StandAwhileWindowClass'
        if ($main -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($main -eq [IntPtr]::Zero) { throw 'Main window not found within 15 seconds.' }
    Start-Sleep -Milliseconds $SettleMs
    Write-Sample 'startup' 0 $main
    for ($cycle = 1; $cycle -le $Cycles; $cycle++) {
        Send-WindowMessage $main 0x111 41004 # Tray Start: production countdown/hide handler.
        $pet = [IntPtr]::Zero
        for ($tick = 0; $tick -le $period; $tick++) {
            $timer = [Diagnostics.Stopwatch]::StartNew()
            Send-WindowMessage $main 0x113 1 # Accelerate countdown; leave animation timers real.
            $timer.Stop()
            $pet = Find-OwnedWindow 'YHB-StandAwhilePetWindow'
            if ($pet -ne [IntPtr]::Zero -and [PetMeasurementNative]::IsWindowVisible($pet)) { break }
        }
        if ($pet -eq [IntPtr]::Zero -or -not [PetMeasurementNative]::IsWindowVisible($pet)) {
            throw "Reminder did not appear in cycle $cycle. Do not change configuration during measurement."
        }
        Start-Sleep -Milliseconds $VisibleMs
        Write-Sample 'visible' $cycle $pet $timer.Elapsed.TotalMilliseconds
        Send-WindowMessage $main 0x111 41004 # Actual animated exit and owner release path.
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while ((Find-OwnedWindow 'YHB-StandAwhilePetWindow') -ne [IntPtr]::Zero) {
            if ([DateTime]::UtcNow -gt $deadline) { throw "Pet was not destroyed in cycle $cycle." }
            Start-Sleep -Milliseconds 50
        }
        Start-Sleep -Milliseconds $SettleMs
        Write-Sample 'released' $cycle $main
    }
} finally {
    # Close only the process launched by this script, through its normal Exit command.
    $app.Refresh()
    if ($main -eq [IntPtr]::Zero -and -not $app.HasExited) {
        $main = Find-OwnedWindow 'YHB-StandAwhileWindowClass'
    }
    if (-not $app.HasExited -and $main -ne [IntPtr]::Zero) {
        Send-WindowMessage $main 0x111 41003
        if (-not $app.WaitForExit(10000)) { Write-Warning 'Application has not exited; close it manually.' }
    } elseif (-not $app.HasExited) {
        Write-Warning "Main window unavailable; close launched PID $($app.Id) manually."
    }
    $app.Dispose()
}
Write-Host "Saved UTF-8 CSV: $Output"
