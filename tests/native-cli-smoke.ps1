# Runs only self-authored six-byte PE fixtures, never game assets.
# CI entry: from the repository root, after a Release build.
param([Parameter(Mandatory=$true)][string]$Runtime)
$ErrorActionPreference = 'Stop'
$runtimePath = (Resolve-Path -LiteralPath $Runtime).ProviderPath
if (-not $env:RUNNER_TEMP) { throw 'R75 requires an isolated CI RUNNER_TEMP' }
$root = Join-Path $env:RUNNER_TEMP ('neptune-r75-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$env:VICTORY_CONFIG = Join-Path $root 'victory.toml'
[IO.File]::WriteAllText($env:VICTORY_CONFIG, "[paths]`n")
$env:VICTORY_GAME_DIR = $root
$env:VICTORY_CACHE_DIR = Join-Path $root 'cache'
$env:VICTORY_DATA_DIR = Join-Path $root 'data'
$env:NEPTUNE_LOG_DIR = Join-Path $root 'logs'

function New-Probe([string]$Path, [uint32]$Value) {
    $b = New-Object byte[] 1024
    function U16([int]$At,[uint16]$V) { [BitConverter]::GetBytes($V).CopyTo($b,$At) }
    function U32([int]$At,[uint32]$V) { [BitConverter]::GetBytes($V).CopyTo($b,$At) }
    function U64([int]$At,[uint64]$V) { [BitConverter]::GetBytes($V).CopyTo($b,$At) }
    U16 0 0x5a4d
    U32 0x3c 0x80
    U32 0x80 0x4550
    U16 0x84 0x8664
    U16 0x86 1
    U16 0x94 240
    U16 0x96 0x23
    $o = 0x98
    U16 $o 0x20b
    U32 ($o+4) 0x200
    U32 ($o+16) 0x1000
    U32 ($o+20) 0x1000
    U64 ($o+24) 0x200000000
    U32 ($o+32) 0x1000
    U32 ($o+36) 0x200
    U16 ($o+40) 6
    U16 ($o+48) 6
    U32 ($o+56) 0x2000
    U32 ($o+60) 0x200
    U16 ($o+68) 3
    U16 ($o+70) 0x100
    U64 ($o+72) 0x100000
    U64 ($o+80) 0x1000
    U64 ($o+88) 0x100000
    U64 ($o+96) 0x1000
    U32 ($o+108) 16
    $s = $o+240
    [Text.Encoding]::ASCII.GetBytes('.text').CopyTo($b,$s)
    U32 ($s+8) 6
    U32 ($s+12) 0x1000
    U32 ($s+16) 0x200
    U32 ($s+20) 0x200
    U32 ($s+36) 0x60000020
    # mov eax, imm32; ret. No imports, I/O, allocations or title data.
    $b[0x200] = 0xb8
    U32 0x201 $Value
    $b[0x205] = 0xc3
    [IO.File]::WriteAllBytes($Path,$b)
}
function Invoke-Probe([string]$Path,[bool]$Execute,[uint32]$Expected) {
    $argv = @('run',$Path,'--strict')
    if ($Execute) { $argv += '--execute' }
    $lines = @(& $runtimePath @argv)
    $code = $LASTEXITCODE
    $lines | ForEach-Object { Write-Host $_ }
    if ($code -ne 0) { throw "R75 CLI exit $code" }
    $entry = @($lines | Where-Object { $_ -match '^title exited with ' })
    if ($Execute) {
        if ($entry.Count -ne 1 -or $entry[0] -cne "title exited with $Expected") {
            throw "R75 native entry mismatch, expected $Expected"
        }
    } else {
        if ($entry.Count -ne 0 -or -not ($lines -contains 'not executing (pass --execute to start the title)')) {
            throw 'R75 bind-only unexpectedly executed or lacked witness'
        }
    }
}
$a = Join-Path $root 'probe-a.exe'
$b = Join-Path $root 'probe-b.exe'
New-Probe $a 42
New-Probe $b 43
$left = [IO.File]::ReadAllBytes($a)
$right = [IO.File]::ReadAllBytes($b)
$diff = @(for ($i=0;$i -lt $left.Length;$i++) { if ($left[$i] -ne $right[$i]) { $i } })
if ($left.Length -ne 1024 -or $right.Length -ne 1024 -or $diff.Count -ne 1 -or $diff[0] -ne 0x201) {
    throw 'R75 fixture byte difference invalid'
}
$before = (Get-FileHash -LiteralPath $runtimePath -Algorithm SHA256).Hash
$hashA = (Get-FileHash -LiteralPath $a -Algorithm SHA256).Hash
$hashB = (Get-FileHash -LiteralPath $b -Algorithm SHA256).Hash
Invoke-Probe $a $false 0
Invoke-Probe $a $true 42
Invoke-Probe $b $true 43
if ((Get-FileHash -LiteralPath $runtimePath -Algorithm SHA256).Hash -cne $before -or
    (Get-FileHash -LiteralPath $a -Algorithm SHA256).Hash -cne $hashA -or
    (Get-FileHash -LiteralPath $b -Algorithm SHA256).Hash -cne $hashB) {
    throw 'R75 input hash drift'
}
Write-Host "R75_RELEASE_SHA256=$before"
Write-Host "R75_FIXTURE_A_SHA256=$hashA"
Write-Host "R75_FIXTURE_B_SHA256=$hashB"
Write-Host 'R75_NATIVE_ENTRY_WITNESS bind_only=1 entry_a=42 entry_b=43 diff_offset=0x201 input_hash_parity=1'
# Generated artifacts remain in runner scratch, never published.
