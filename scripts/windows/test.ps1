param(
    [Parameter(Mandatory = $true)]
    [string]$Checks,
    [string]$RustTests = ''
)

$ErrorActionPreference = 'Stop'
$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$env:NO_COLOR = '1'
$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $projectRoot
$startedAt = (Get-Date).ToUniversalTime().ToString('o')
$defaultChecks = @('rust', 'frontend', 'tauri-build')
$allowed = $defaultChecks + @('rust-check', 'service-build')
$requested = if ($Checks -eq 'all') { $defaultChecks } else { $Checks.Split(',') | ForEach-Object { $_.Trim() } }

if ($requested.Count -eq 0 -or ($requested | Where-Object { $_ -notin $allowed }).Count -gt 0) {
    $result = [ordered]@{ schemaVersion = 1; platform = 'windows'; status = 'blocked'; requestedChecks = @($requested); checks = @(); manual = @{ status = 'not-reached'; items = @() }; error = @{ code = 'invalid_check'; message = 'Unsupported Windows check requested' } }
    Write-Output ("VNC_WINDOWS_RESULT=" + ($result | ConvertTo-Json -Compress -Depth 6))
    exit 30
}

$rustTargets = if ([string]::IsNullOrEmpty($RustTests)) { @() } else { @($RustTests.Split(',')) }
if (($PSBoundParameters.ContainsKey('RustTests') -and $rustTargets.Count -eq 0) -or ($rustTargets.Count -gt 0 -and (
    'rust' -notin $requested -or $rustTargets.Count -gt 32 -or
    @($rustTargets | Where-Object { $_.Length -gt 64 -or $_ -notmatch '^[A-Za-z0-9_][A-Za-z0-9_-]*$' }).Count -gt 0
))) {
    $result = [ordered]@{ schemaVersion = 1; platform = 'windows'; status = 'blocked'; requestedChecks = @($requested); checks = @(); manual = @{ status = 'not-reached'; items = @() }; error = @{ code = 'invalid_rust_tests'; message = 'Invalid Rust test target selection' } }
    Write-Output ("VNC_WINDOWS_RESULT=" + ($result | ConvertTo-Json -Compress -Depth 6))
    exit 30
}

function Get-SafeOutput([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
    $safe = $Text -replace '(?i)(password|passwd|token|secret|api[_-]?key)\s*[=:]\s*[^\s]+', '$1=[redacted]'
    if ($safe.Length -gt 4000) { $safe = $safe.Substring($safe.Length - 4000) }
    return $safe
}

function Invoke-SelectedCheck([string]$Name) {
    $started = Get-Date
    $output = ''
    $exitCode = 1
    try {
        $commandNames = @('cargo.exe', 'cargo.cmd')
        $commandArgs = @(switch ($Name) {
            'rust' {
                @('test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml')
                foreach ($target in $rustTargets) {
                    if ($target -eq 'lib') { '--lib' }
                    else { '--test'; $target }
                }
            }
            'rust-check' { @('check', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--all-targets') }
            'service-build' { @('build', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--bin', 'LLRemoteService') }
            'frontend' { @('test:ci') }
            'tauri-build' { @('tauri', 'build') }
        })
        if ($Name -in @('frontend', 'tauri-build')) { $commandNames = @('pnpm.exe', 'pnpm.cmd') }
        $command = $null
        foreach ($candidate in $commandNames) {
            $command = Get-Command $candidate -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($null -ne $command) { break }
        }
        if ($null -eq $command) { throw "runner_command_unavailable: $($commandNames -join ',')" }
        # Native stderr is captured as evidence; only this invocation's exit code counts.
        $global:LASTEXITCODE = $null
        $ErrorActionPreference = 'Continue'
        $output = & $command.Source @commandArgs 2>&1 | Out-String
        if ($null -eq $global:LASTEXITCODE) { throw 'runner_native_exit_code_missing' }
        $exitCode = $global:LASTEXITCODE
    } catch {
        $output = $output + [Environment]::NewLine + $_.Exception.Message
        $exitCode = 1
    }
    $entry = [ordered]@{ command = @{ executable = $command.Source; arguments = @($commandArgs) }; name = $Name; status = if ($exitCode -eq 0) { 'passed' } else { 'failed' } }
    if ($Name -eq 'rust') {
        $entry.rustTests = @($rustTargets)
        $entry.testResults = @([regex]::Matches($output, 'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out') | ForEach-Object {
            @{ passed = [int]$_.Groups[2].Value; failed = [int]$_.Groups[3].Value; ignored = [int]$_.Groups[4].Value; measured = [int]$_.Groups[5].Value; filteredOut = [int]$_.Groups[6].Value }
        })
    }
    if ($Name -eq 'frontend') {
        $counts = @{}
        foreach ($field in @('tests', 'pass', 'fail', 'cancelled', 'skipped')) {
            $match = [regex]::Match($output, "(?m)^# $field ([0-9]+)\s*$")
            if ($match.Success) { $counts[$field] = [int]$match.Groups[1].Value }
        }
        if ($counts.Count -eq 5) {
            $entry.testResults = @{ total = $counts.tests; passed = $counts.pass; failed = $counts.fail; cancelled = $counts.cancelled; skipped = $counts.skipped }
        }
        if ($exitCode -eq 0 -and ($counts.Count -ne 5 -or $counts.pass -eq 0 -or $counts.fail -ne 0 -or $counts.cancelled -ne 0)) {
            $exitCode = 1; $output += [Environment]::NewLine + 'runner_test_evidence_missing_or_failed'
        }
    }
    if ($Name -eq 'rust' -and $exitCode -eq 0) {
        $passed = 0; $failedCount = 0
        foreach ($summary in $entry.testResults) {
            $passed += $summary.passed
            $failedCount += $summary.failed
        }
        if ($entry.testResults.Count -eq 0 -or $passed -eq 0 -or $failedCount -ne 0) {
            $exitCode = 1; $output += [Environment]::NewLine + 'runner_test_evidence_missing_or_failed'
        }
    }
    $entry.status = if ($exitCode -eq 0) { 'passed' } else { 'failed' }
    if ($exitCode -ne 0) { $entry.exitCode = $exitCode; $entry.error = Get-SafeOutput $output; $entry.outputStart = Get-SafeOutput ($output.Substring(0, [Math]::Min(600, $output.Length))) }
    $entry.durationMs = [int]((Get-Date) - $started).TotalMilliseconds
    return $entry
}

$entries = @()
foreach ($check in $requested) {
    [Console]::Error.WriteLine("[windows] running $check")
    $entries += Invoke-SelectedCheck $check
}
$failed = @($entries | Where-Object { $_.status -eq 'failed' }).Count -gt 0
$result = [ordered]@{
    schemaVersion = 1; platform = 'windows'; status = if ($failed) { 'failed' } else { 'passed' }
    requestedChecks = @($requested); checks = @($entries)
    manual = @{ status = 'pending'; items = @('Tauri 窗口启动', 'VNC 连接', '键盘与剪贴板') }
    error = if ($failed) { @{ code = 'check_failed'; message = 'One or more selected checks failed' } } else { $null }
    startedAt = $startedAt; finishedAt = (Get-Date).ToUniversalTime().ToString('o')
}
Write-Output ("VNC_WINDOWS_RESULT=" + ($result | ConvertTo-Json -Compress -Depth 8))
exit $(if ($failed) { 10 } else { 0 })
