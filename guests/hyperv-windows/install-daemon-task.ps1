<#
.SYNOPSIS
Prepares the guest to run the guest daemon in the interactive session of the user who logs on automatically.

.DESCRIPTION
Opens TCP 7041 inbound and registers the scheduled task vmtest-guest-daemon, which runs C:\vmtest\vmtest-guest-daemon.exe
in that user's session, at the user's integrity level, with no trigger. The harness copies the executable and starts
the task on each run. Runs elevated, over SSH as an administrator.

.PARAMETER User
The user who logs on automatically.
#>
param(
    [Parameter(Mandatory)] [string] $User
)
$ErrorActionPreference = 'Stop'

if (-not (Get-NetFirewallRule -Name 'vmtest-guest-daemon' -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -Name 'vmtest-guest-daemon' -DisplayName 'vmtest guest daemon' `
        -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 7041 -Profile Any | Out-Null
}

$action = New-ScheduledTaskAction -Execute 'C:\vmtest\vmtest-guest-daemon.exe'
$principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName 'vmtest-guest-daemon' -Action $action -Principal $principal -Force | Out-Null
