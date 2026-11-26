param()

# Isolated filesystem fixtures only: this script never launches an installer,
# registers a service or touches the signed-in user's actual application data.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$OutputEncoding = [Console]::OutputEncoding = [Text.UTF8Encoding]::new()
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root
$cargo = (Get-Command cargo.exe -CommandType Application).Source
$manifest = Join-Path $root 'src-tauri\windows\app.manifest'
$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$mt = Get-ChildItem $kits -Filter mt.exe -Recurse | Where-Object FullName -match '\\x64\\mt.exe$' | Sort-Object FullName -Descending | Select-Object -First 1
if (!$mt) { throw 'Windows SDK manifest tool is required for native test executables.' }

function Build-Test([string[]]$Targets) {
    # Cargo's library/integration test executables do not inherit the Tauri
    # application resource. TaskDialogIndirect requires Common Controls v6 even
    # when the filesystem test itself never opens a dialog.
    $arguments = @('test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--no-run', '--message-format=json') + $Targets
    $ErrorActionPreference = 'Continue'
    $lines = @(& $cargo @arguments 2>&1 | ForEach-Object { $_.ToString() })
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code -ne 0) { $lines | Write-Output; throw "Test build failed: $code" }
    $executables = @($lines | Where-Object { $_.StartsWith('{') } | ForEach-Object {
        $item = ConvertFrom-Json $_
        if ($item.reason -eq 'compiler-artifact' -and $item.profile.test -and $item.executable) { $item.executable }
    } | Select-Object -Unique)
    if ($executables.Count -ne 1) { throw 'Expected exactly one native test executable.' }
    $exe = $executables[0]
    & $mt.FullName '-nologo' '-manifest' $manifest ('-outputresource:' + $exe + ';#1') | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Embedding the application manifest failed: $LASTEXITCODE" }
    return $exe
}

$previousConfig = $env:TAURI_CONFIG
try {
    $env:TAURI_CONFIG = Get-Content -Raw (Join-Path $root 'src-tauri\tauri.windows.conf.json')
    & $cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
    if ($LASTEXITCODE -ne 0) { throw "Windows compilation failed: $LASTEXITCODE" }
    $library = Build-Test @('--lib')
    & $library 'windows_uninstall::data' --nocapture
    if ($LASTEXITCODE -ne 0) { throw "Data cleanup tests failed: $LASTEXITCODE" }
    $access = Build-Test @('--test', 'file_access_tests')
    & $access --nocapture
    if ($LASTEXITCODE -ne 0) { throw "File access tests failed: $LASTEXITCODE" }
    Write-Output 'LLREMOTE_STORAGE_VALIDATION=passed'
} finally {
    $env:TAURI_CONFIG = $previousConfig
}
