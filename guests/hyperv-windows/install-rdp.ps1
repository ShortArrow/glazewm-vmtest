<#
.SYNOPSIS
Lets an RDP client log on to the guest, and lets the harness return the session to the console afterwards.

.DESCRIPTION
Allows Remote Desktop connections but enables no firewall rule for them, so the RDP port is reachable only from the
guest itself; the harness reaches it through an SSH port forward. Registers the scheduled task vmtest-tscon, which
runs as SYSTEM and moves the user's session to the console with tscon, as a user does by logging on at the machine
after a disconnect. Runs elevated, over SSH as an administrator.

.PARAMETER User
The user who logs on automatically, whose session the task moves.
#>
param(
    [Parameter(Mandatory)] [string] $User
)
$ErrorActionPreference = 'Stop'

Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' -Name fDenyTSConnections -Value 0

$moveToConsole = "`$line = quser 2>`$null | Where-Object { `$_ -match '^\s*>?$User\s' }; " +
    "if (`$line -match '\s(\d+)\s+(Active|Disc)') { tscon `$Matches[1] /dest:console }"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -NonInteractive -Command `"$moveToConsole`""
$principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
Register-ScheduledTask -TaskName 'vmtest-tscon' -Action $action -Principal $principal -Force | Out-Null

$open = Get-NetFirewallRule -Group '@FirewallAPI.dll,-28752' | Where-Object Enabled -eq 'True'
if ($open) { throw "Remote Desktop firewall rules are enabled: $($open.Name -join ', ')" }
