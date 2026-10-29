<#
.SYNOPSIS
Prepares the guest to run the guest daemon in the interactive session of the user who logs on automatically.

.DESCRIPTION
Registers the scheduled task vmtest-guest-daemon, which runs C:\vmtest\vmtest-guest-daemon.exe in that user's session,
at the user's integrity level, with no trigger. The harness copies the executable and starts the task on each run.
The daemon has no authentication and can make glazewm run commands, so it listens on the guest's loopback only and no
firewall rule opens it; the harness reaches it through an SSH port forward. Runs elevated, over SSH as an administrator.

.PARAMETER User
The user who logs on automatically.
#>
param(
    [Parameter(Mandatory)] [string] $User
)
$ErrorActionPreference = 'Stop'

$action = New-ScheduledTaskAction -Execute 'C:\vmtest\vmtest-guest-daemon.exe' -Argument '127.0.0.1:7041'
$principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName 'vmtest-guest-daemon' -Action $action -Principal $principal -Force | Out-Null
