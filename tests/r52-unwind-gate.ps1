# R61 — Native R52 unwind safety gate. Windows PowerShell 5.1.
# Original test harness only. Never changes source, title or R50 checkpoint.
param(
    [Parameter(Mandatory=$true)][ValidateSet('targeted','gpu','workspace','physical')][string]$Phase,
    [Parameter(Mandatory=$true)][string]$SourceRoot,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][string]$TargetRoot,
    [Parameter(Mandatory=$true)][string]$ProtectedR50,
    [Parameter(Mandatory=$true)][string]$ExpectedR50,
    [Parameter(Mandatory=$true)][string]$ExpectedProfile,
    [Parameter(Mandatory=$true)][string]$ExpectedTests
)
Set-StrictMode -Version 2.0
$ErrorActionPreference='Stop'
$src=[IO.Path]::GetFullPath((Resolve-Path -LiteralPath $SourceRoot).Path)
$profile=Join-Path $src 'crates\victory-gpu\src\profile.rs'
$tests=Join-Path $src 'crates\victory-gpu\src\profile\tests.rs'
$protected=[IO.Path]::GetFullPath((Resolve-Path -LiteralPath $ProtectedR50).Path)
$out=[IO.Path]::GetFullPath($OutputRoot)
$target=[IO.Path]::GetFullPath($TargetRoot)
$prefix=$src.TrimEnd([char]'\',[char]'/')+'\'
foreach($path in @($out,$target)) {
    if($path -eq $src -or $path.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) {
        throw 'DESTINATION_INSIDE_SOURCE'
    }
}
if($out -eq $target -or $out.StartsWith($target+'\',[StringComparison]::OrdinalIgnoreCase) -or
   $target.StartsWith($out+'\',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'OUTPUT_TARGET_OVERLAP'
}
foreach($pair in @(@($protected,$ExpectedR50),@($profile,$ExpectedProfile),@($tests,$ExpectedTests))) {
    if($pair[1] -notmatch '^[a-fA-F0-9]{64}$') {throw 'EXPECTED_SHA256_REQUIRED'}
    $got=(Get-FileHash -LiteralPath $pair[0] -Algorithm SHA256).Hash.ToLowerInvariant()
    if($got -cne $pair[1].ToLowerInvariant()) {throw ('PREIMAGE_DRIFT '+$pair[0])}
}
if(!(Test-Path -LiteralPath $out)){[void](New-Item -ItemType Directory -Path $out)}
if(!(Test-Path -LiteralPath $target)){[void](New-Item -ItemType Directory -Path $target)}
$env:CARGO_TARGET_DIR=$target
$env:CARGO_TERM_COLOR='never'
$env:GIT_TERMINAL_PROMPT='0'
$env:RUST_BACKTRACE='0'
$cargo=(Get-Command cargo.exe -ErrorAction Stop).Source
$cases=@()
switch($Phase) {
 'targeted' {
    $cases=@(
        @{Id='bounded';Name='profile::tests::neptune_r52_bounded_unwind_fails_closed';Profile='diag';Ignored=$false},
        @{Id='rpc-valid';Name='profile::tests::neptune_r52_rpc_valid_boundary';Profile='diag';Ignored=$true},
        @{Id='rpc-mid';Name='profile::tests::neptune_r52_rpc_mid_instruction';Profile='diag';Ignored=$true},
        @{Id='stress';Name='profile::tests::stacks_that_mean_nothing_are_walked_without_a_fault';Profile='diag';Ignored=$false}
    )
 }
 'gpu' { $cases=@(@{Id='gpu-diag';Name='';Profile='diag';Ignored=$false}) }
 'workspace' { $cases=@(@{Id='workspace-release';Name='';Profile='release';Ignored=$false}) }
 'physical' {
    $cases=@(@{Id='gpu-prefix';Name='texture::title_memory_tests::neptune_physical_prefix_gpu_roundtrip';Profile='diag';Ignored=$true})
 }
}
Write-Output ('R61_PHASE='+$Phase+' R50_SHA256='+$ExpectedR50.ToLowerInvariant())
Write-Output ('R61_PROFILE_SHA256='+$ExpectedProfile.ToLowerInvariant())
Write-Output ('R61_TESTS_SHA256='+$ExpectedTests.ToLowerInvariant())
$complete=0
foreach($case in $cases){
    $stdout=Join-Path $out ('r61-'+$case.Id+'.stdout.txt')
    $stderr=Join-Path $out ('r61-'+$case.Id+'.stderr.txt')
    if((Test-Path -LiteralPath $stdout) -or (Test-Path -LiteralPath $stderr)){throw ('LOG_EXISTS '+$case.Id)}
    $argv=@('test','--locked','--offline','--profile',$case.Profile)
    if($Phase -eq 'workspace') {$argv+=@('--workspace','--all-features')}
    else {$argv+=@('-p','victory-gpu','--features','diagnostics','--lib')}
    if($case.Name -ne ''){$argv+=@($case.Name)}
    $argv+='--'
    if($case.Ignored){$argv+='--ignored'}
    if($case.Name -ne ''){$argv+='--exact'}
    $argv+=@('--nocapture','--test-threads=1')
    if($Phase -eq 'physical'){$env:VICTORY_TILES_TEST_CARD='1'}
    else {Remove-Item Env:\VICTORY_TILES_TEST_CARD -ErrorAction SilentlyContinue}
    Write-Output ('R61_BEGIN='+$case.Id+' ARGV='+($argv -join ' '))
    $p=Start-Process -FilePath $cargo -ArgumentList $argv -WorkingDirectory $src -Wait -NoNewWindow -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $text=[IO.File]::ReadAllText($stdout)
    $results=@([regex]::Matches($text,'test result: ok\. [0-9]+ passed; 0 failed; [0-9]+ ignored'))
    if($case.Name -ne ''){
        $name=[regex]::Escape($case.Name)
        $ok=($results.Count -eq 1 -and
            $text -match 'test result: ok\. 1 passed; 0 failed; 0 ignored' -and
            $text -match ('test '+$name+' \.\.\. '))
        if($case.Id -eq 'stress'){$ok=$ok -and $text.Contains('60000 walks')}
        if($case.Id -eq 'gpu-prefix'){$ok=$ok -and $text.Contains('ADAPTER=10de:2882') -and $text.Contains('MISMATCHES=0')}
    }
    else {$ok=($results.Count -ge 1 -and !$text.Contains('test result: FAILED') -and $text.Contains('test result: ok. '))}
    $hashout=(Get-FileHash -LiteralPath $stdout -Algorithm SHA256).Hash.ToLowerInvariant()
    $hasherr=(Get-FileHash -LiteralPath $stderr -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output ('R61_RESULT='+$case.Id+' EXIT='+$p.ExitCode+' VALID='+$ok+' STDOUT='+$hashout+' STDERR='+$hasherr+' BLOCKS='+$results.Count)
    if($p.ExitCode -ne 0 -or !$ok){throw ('R61_CASE_FAILED '+$case.Id)}
    $complete++
}
foreach($pair in @(@($protected,$ExpectedR50),@($profile,$ExpectedProfile),@($tests,$ExpectedTests))) {
    if((Get-FileHash -LiteralPath $pair[0] -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pair[1].ToLowerInvariant()){
        throw ('POSTIMAGE_DRIFT '+$pair[0])
    }
}
Write-Output ('R61_PHASE_PASS='+$Phase+' CASES='+$complete+' PREPOST_HASH_PARITY=1')
