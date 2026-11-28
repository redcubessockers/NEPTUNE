# Windows PowerShell 5.1. Native release acceptance only; no title files.
param(
    [ValidateSet('landing','retirement','descriptor','relay','partial','roundtrip','sparse')]
    [string[]]$Cases=@('landing','retirement','descriptor','relay','partial','roundtrip','sparse'),
    [switch]$IncludeWorker,
    [string]$OutputRoot='',
    [string]$ProtectedFile='',
    [string]$ExpectedSha256=''
)
Set-StrictMode -Version 2.0
$ErrorActionPreference='Stop'

$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$head=(& git -C $repo rev-parse HEAD).Trim()
if($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-f]{40}$'){throw 'GIT_HEAD_UNKNOWN'}
& git -C $repo diff --quiet
if($LASTEXITCODE -ne 0){throw 'TRACKED_SOURCE_DIRTY'}
& git -C $repo diff --cached --quiet
if($LASTEXITCODE -ne 0){throw 'STAGED_SOURCE_DIRTY'}

if($ProtectedFile -ne ''){
    if($ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$'){throw 'EXPECTED_PROTECTED_SHA256_REQUIRED'}
    $protected=[IO.Path]::GetFullPath($ProtectedFile)
    if(!(Test-Path -LiteralPath $protected -PathType Leaf)){throw 'PROTECTED_FILE_MISSING'}
    $protectedBefore=(Get-FileHash -LiteralPath $protected -Algorithm SHA256).Hash.ToLowerInvariant()
    if($protectedBefore -cne $ExpectedSha256.ToLowerInvariant()){throw 'PROTECTED_FILE_DRIFT_BEFORE'}
}
elseif($ExpectedSha256 -ne ''){throw 'PROTECTED_FILE_REQUIRED'}

if($OutputRoot -eq ''){
    $OutputRoot=Join-Path $env:TEMP ('NEPTUNE-HARDWARE-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmss')+'-'+$PID)
}
$output=[IO.Path]::GetFullPath($OutputRoot)
$sourcePrefix=$repo.TrimEnd([char]'\',[char]'/')+'\'
if($output.StartsWith($sourcePrefix,[StringComparison]::OrdinalIgnoreCase) -or
   $output -eq $repo){throw 'LOGS_MUST_BE_OUTSIDE_SOURCE'}
if(Test-Path -LiteralPath $output){throw 'OUTPUT_COLLISION'}
[void](New-Item -ItemType Directory -Path $output)

$tests=@{
    landing='neptune_physical_landing_fence_boundary'
    retirement='neptune_physical_retirement_admission'
    descriptor='neptune_physical_descriptor_lifetime'
    relay='neptune_physical_relay_prefix'
    partial='neptune_physical_partial_refresh'
    roundtrip='neptune_physical_prefix_gpu_roundtrip'
    sparse='neptune_physical_sparse_mips'
    worker='neptune_worker_join_failure_remains_failure'
}
$selected=@($Cases)
if($IncludeWorker){$selected+= 'worker'}
if($selected.Count -eq 0){throw 'EMPTY_SELECTION'}
$env:CARGO_TERM_COLOR='never'
$env:CARGO_TARGET_DIR=Join-Path $repo 'target'
if(!(Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){
    [void](New-Item -ItemType Directory -Path $env:CARGO_TARGET_DIR)
}
$cargo=(Get-Command cargo.exe).Source
Write-Output ('SOURCE_HEAD='+$head)
Write-Output ('RAW_OUTPUT='+$output)
Write-Output ('PROTECTED_FILE=' + $(if($ProtectedFile -eq ''){'NOT_CONFIGURED'}else{'PINNED'}))
Write-Output ('TEST_COUNT='+$selected.Count)
foreach($case in $selected){
    $out=Join-Path $output ($case+'.stdout.txt')
    $err=Join-Path $output ($case+'.stderr.txt')
    if($case -in @('roundtrip','sparse')){$env:VICTORY_TILES_TEST_CARD='1'}
    else{Remove-Item Env:\VICTORY_TILES_TEST_CARD -ErrorAction SilentlyContinue}
    $argv=@('test','--locked','--offline','--release','-p','victory-gpu','--lib',
            $tests[$case],'--','--ignored','--nocapture','--test-threads=1')
    $proc=Start-Process -FilePath $cargo -ArgumentList $argv -WorkingDirectory $repo -NoNewWindow -Wait -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
    $text=[IO.File]::ReadAllText($out)
    $matched=($text -match 'test result: ok\. 1 passed; 0 failed; 0 ignored')
    if($case -eq 'worker'){$matched=$matched -and ($text -match 'R54_FIRST_JOIN=false R54_SECOND_JOIN=false')}
    else{$matched=$matched -and ($text -match 'ADAPTER=10de:')}
    $stdoutHash=(Get-FileHash -LiteralPath $out -Algorithm SHA256).Hash.ToLowerInvariant()
    $stderrHash=(Get-FileHash -LiteralPath $err -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output ('TEST='+$case+' EXIT='+$proc.ExitCode+' MATCH='+$matched+' STDOUT_SHA256='+$stdoutHash+' STDERR_SHA256='+$stderrHash)
    if($ProtectedFile -ne ''){
        $protectedAfter=(Get-FileHash -LiteralPath $protected -Algorithm SHA256).Hash.ToLowerInvariant()
        if($protectedAfter -cne $protectedBefore){throw 'PROTECTED_FILE_DRIFT_AFTER'}
    }
    if($proc.ExitCode -ne 0 -or !$matched){throw ('TEST_FAILED_'+$case)}
}
& git -C $repo diff --quiet
if($LASTEXITCODE -ne 0){throw 'SOURCE_DIRTY_AFTER'}
& git -C $repo diff --cached --quiet
if($LASTEXITCODE -ne 0){throw 'SOURCE_STAGED_AFTER'}
if(((& git -C $repo rev-parse HEAD).Trim()) -cne $head){throw 'HEAD_CHANGED'}
Write-Output ('GATE=PASS TESTS='+$selected.Count+' SOURCE_CLEAN=1')
