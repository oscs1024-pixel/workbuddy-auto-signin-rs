# WorkBuddy Auto Sign-in Rust installer for Windows Task Scheduler.
$ErrorActionPreference = "Stop"

$ManualBinary = "" # Optional full path to workbuddy-auto-signin.exe

function Find-Binary {
    if ($ManualBinary -and (Test-Path $ManualBinary)) { return (Resolve-Path $ManualBinary).Path }
    if ($PSScriptRoot) {
        $repo = Split-Path $PSScriptRoot -Parent
        foreach ($candidate in @(
            (Join-Path $repo "workbuddy-auto-signin.exe"),
            (Join-Path $repo "target\release\workbuddy-auto-signin.exe")
        )) {
            if (Test-Path $candidate) { return (Resolve-Path $candidate).Path }
        }
    }
    $cmd = Get-Command workbuddy-auto-signin.exe -ErrorAction SilentlyContinue
    if ($cmd -and (Test-Path $cmd.Source)) { return $cmd.Source }
    return $null
}

Write-Host "WorkBuddy Auto Sign-in (Rust)" -ForegroundColor Cyan
$binary = Find-Binary
if (-not $binary) {
    Write-Host "Cannot find workbuddy-auto-signin.exe." -ForegroundColor Red
    Write-Host "Build with 'cargo build --release' or set `$ManualBinary at the top of this script." -ForegroundColor Yellow
    exit 1
}
Write-Host "Binary: $binary" -ForegroundColor Green

$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive

$act1 = New-ScheduledTaskAction -Execute $binary -Argument "silent"
$tri1 = New-ScheduledTaskTrigger -Daily -At "00:05"
$set1 = New-ScheduledTaskSettingsSet -StartWhenAvailable -Hidden `
        -DontStopIfGoingOnBatteries -AllowStartIfOnBatteries `
        -ExecutionTimeLimit (New-TimeSpan -Minutes 10)
Register-ScheduledTask -TaskName "WorkBuddyAutoSignin" `
    -Action $act1 -Trigger $tri1 -Settings $set1 -Principal $principal `
    -Description "WorkBuddy daily auto signin (Rust, silent)" -Force | Out-Null

$tri2 = @()
foreach ($hh in @("01:00", "05:00", "09:00", "13:00", "17:00", "21:00")) {
    $tri2 += New-ScheduledTaskTrigger -Daily -At $hh
}
$act2 = New-ScheduledTaskAction -Execute $binary -Argument "silent-poll"
$set2 = New-ScheduledTaskSettingsSet -StartWhenAvailable -Hidden `
        -DontStopIfGoingOnBatteries -AllowStartIfOnBatteries `
        -ExecutionTimeLimit (New-TimeSpan -Minutes 5)
Register-ScheduledTask -TaskName "WorkBuddyGrowthPoll" `
    -Action $act2 -Trigger $tri2 -Settings $set2 -Principal $principal `
    -Description "WorkBuddy catch-up signin + growth center (Rust)" -Force | Out-Null

Write-Host "Installed WorkBuddyAutoSignin and WorkBuddyGrowthPoll." -ForegroundColor Green
Write-Host "Log defaults to signin.log next to the executable (override with WORKBUDDY_SIGNIN_LOG)."
