<#
.SYNOPSIS
Turns a Hyper-V guest off, restores it to a checkpoint, and starts it, through WMI alone.

.DESCRIPTION
Uses only the classes of root\virtualization\v2, so the Hyper-V PowerShell module is not needed.
Returns once the guest is running; whether the guest is usable is for the caller to find out through the guest.
Needs membership of Administrators or Hyper-V Administrators on the Hyper-V host.
#>
param(
    [Parameter(Mandatory)] [string] $VMName,
    [Parameter(Mandatory)] [string] $Checkpoint
)
$ErrorActionPreference = 'Stop'
$ns = 'root\virtualization\v2'
$running = 2
$off = 3

function Get-Guest {
    $guest = Get-CimInstance -Namespace $ns -ClassName Msvm_ComputerSystem -Filter "ElementName='$($VMName -replace "'", "''")'"
    if (-not $guest) { throw "no Hyper-V guest named '$VMName'" }
    $guest
}

function Complete($Result, [string] $What) {
    if ($Result.ReturnValue -eq 4096) {
        do {
            Start-Sleep -Milliseconds 100
            $job = Get-CimInstance -Namespace $ns -ClassName Msvm_ConcreteJob -Filter "InstanceID='$($Result.Job.InstanceID)'"
        } while ($job.JobState -lt 7)
        if ($job.JobState -ne 7) { throw "$What failed with job state $($job.JobState): $($job.ErrorDescription)" }
    } elseif ($Result.ReturnValue -ne 0) {
        throw "$What returned $($Result.ReturnValue)"
    }
}

function Set-GuestState([uint16] $State) {
    if ((Get-Guest).EnabledState -eq $State) { return }
    Complete (Invoke-CimMethod -InputObject (Get-Guest) -MethodName RequestStateChange -Arguments @{ RequestedState = $State }) "changing '$VMName' to state $State"
    while ((Get-Guest).EnabledState -ne $State) { Start-Sleep -Milliseconds 100 }
}

$snapshot = Get-CimAssociatedInstance -InputObject (Get-Guest) -Association Msvm_SnapshotOfVirtualSystem -ResultClassName Msvm_VirtualSystemSettingData |
    Where-Object ElementName -eq $Checkpoint
if (@($snapshot).Count -ne 1) { throw "'$VMName' has $(@($snapshot).Count) checkpoints named '$Checkpoint'" }
Set-GuestState $off
$service = Get-CimInstance -Namespace $ns -ClassName Msvm_VirtualSystemSnapshotService
Complete (Invoke-CimMethod -InputObject $service -MethodName ApplySnapshot -Arguments @{ Snapshot = $snapshot }) "restoring '$Checkpoint'"
Set-GuestState $running
