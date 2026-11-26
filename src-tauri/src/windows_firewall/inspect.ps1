$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
    if ($domainJoined -isnot [bool]) { throw 'domain membership must be supplied by the native query' }
    # COM reads are available to non-elevated desktop callers. Never request
    # runas or change provider permissions just to display system status.
    $policy = $null
    try { $policy = New-Object -ComObject HNetCfg.FwPolicy2 } catch { }
    # Read the two dimensions independently; a failed query must not look like
    # a disconnected network or a disabled firewall.
    $connectionsKnown = $false
    $connected = @{}
    try {
        foreach ($network in @(Get-NetConnectionProfile)) {
            if ([string]$network.IPv4Connectivity -eq 'Disconnected' -and
                [string]$network.IPv6Connectivity -eq 'Disconnected') { continue }
            switch ([string]$network.NetworkCategory) {
                'DomainAuthenticated' { $connected['domain'] = $true }
                'Private' { $connected['private'] = $true }
                'Public' { $connected['public'] = $true }
            }
        }
        $connectionsKnown = $true
    } catch {
        if ($null -ne $policy) {
            try {
                $active = [int]$policy.CurrentProfileTypes
                $connected = @{}
                if (($active -band 1) -ne 0) { $connected['domain'] = $true }
                if (($active -band 2) -ne 0) { $connected['private'] = $true }
                if (($active -band 4) -ne 0) { $connected['public'] = $true }
                $connectionsKnown = $true
            } catch { }
        }
    }
    $effectiveProfiles = @()
    try { $effectiveProfiles = @(Get-NetFirewallProfile -PolicyStore ActiveStore) } catch { }
    $profiles = @()
    foreach ($entry in @(@('domain', 'Domain', 1), @('private', 'Private', 2), @('public', 'Public', 4))) {
        # All Windows installations have three policy templates. The domain
        # template isn't applicable to a workgroup computer. A disconnected
        # private/public network still belongs in the native panel's list.
        if ($entry[0] -eq 'domain' -and !$domainJoined -and !$connected.ContainsKey('domain')) { continue }
        $enabled = $null
        $matching = @($effectiveProfiles | Where-Object { [string]$_.Name -eq $entry[1] })
        if ($matching.Count -eq 1) {
            switch ([string]$matching[0].Enabled) {
                'True' { $enabled = $true }
                'False' { $enabled = $false }
            }
        }
        if ($null -eq $enabled -and $null -ne $policy) {
            # FirewallEnabled is a local setting. Honor the documented GPO
            # override when present, and don't guess through an unreadable or
            # overridden policy. ActiveStore above remains authoritative.
            $policyKey = $null
            try {
                $policyKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
                    ('SOFTWARE\Policies\Microsoft\WindowsFirewall\' + $entry[1] + 'Profile'), $false)
                $override = $null
                if ($null -ne $policyKey) { $override = $policyKey.GetValue('EnableFirewall', $null) }
                if ($null -ne $override) {
                    if ($override -eq 0) { $enabled = $false }
                    elseif ($override -eq 1) { $enabled = $true }
                } elseif ([int]$policy.LocalPolicyModifyState -eq 0) {
                    $enabled = [bool]$policy.FirewallEnabled([int]$entry[2])
                }
            } catch { }
            finally { if ($null -ne $policyKey) { $policyKey.Dispose() } }
        }
        $isConnected = $null
        if ($connectionsKnown) { $isConnected = [bool]$connected.ContainsKey($entry[0]) }
        $profiles += @{ name = $entry[0]; connected = $isConnected; enabled = $enabled }
    }
    @{ profiles = @($profiles) } | ConvertTo-Json -Depth 4 -Compress
} catch { exit 1 }
