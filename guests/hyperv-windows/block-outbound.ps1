<#
.SYNOPSIS
Blocks every outbound connection from the guest, so that no update or download starts in the guest while a
scenario runs.

.DESCRIPTION
Sets the default outbound action of the domain, private and public firewall profiles to Block. Inbound connections,
the replies on them and connections on the loopback address are not affected: SSH, its port forwards to the guest
daemon, and glazewm's IPC keep working. Fails unless a connection to a public address is refused afterwards. Runs
elevated, over SSH as an administrator.
#>
$ErrorActionPreference = 'Stop'

Set-NetFirewallProfile -Profile Domain, Private, Public -DefaultOutboundAction Block -Enabled True

$client = New-Object System.Net.Sockets.TcpClient
try {
    $connected = $client.ConnectAsync('1.1.1.1', 443).Wait(5000) -and $client.Connected
} catch {
    $connected = $false
} finally {
    $client.Dispose()
}
if ($connected) { throw 'An outbound connection to 1.1.1.1:443 still succeeds.' }
'Outbound connections are blocked.'
