<#
.SYNOPSIS
Idle memory and CPU of a running Bolo (M4 ticket 04; BENCH.md §21).

.DESCRIPTION
Finds the app's process by name (and optionally by path, when Bolo and Bolo Nightly both
run), then walks ParentProcessId down from it to collect its msedgewebview2.exe
descendants. Prints private bytes and working set for the app and for its WebView2
processes, and CPU over -Seconds (the share of the whole machine, all cores).

Read-only: it never starts, stops or sends input to anything. Run it with the model
unloaded (about 60 s after the last dictation) and every Bolo window closed.

.EXAMPLE
powershell -NoProfile -File scripts/idle_ram.ps1
powershell -NoProfile -File scripts/idle_ram.ps1 -PathLike '*Bolo Nightly*'
powershell -NoProfile -File scripts/idle_ram.ps1 -ProcessId 1234 -Seconds 30
#>
param(
    [string]$Name = 'bolo-desktop',
    # A -like pattern on the exe path, to pick one of several processes with that name.
    [string]$PathLike = '*',
    [int]$ProcessId = 0,
    [int]$Seconds = 10
)
$ErrorActionPreference = 'Stop'

$all = Get-CimInstance Win32_Process
if ($ProcessId) {
    $roots = @($all | Where-Object { $_.ProcessId -eq $ProcessId })
} else {
    $roots = @($all | Where-Object { $_.Name -eq "$Name.exe" -and $_.ExecutablePath -like $PathLike })
}
if ($roots.Count -ne 1) {
    $roots | ForEach-Object { Write-Host "  $($_.ProcessId)  $($_.ExecutablePath)" }
    throw "expected one $Name process matching '$PathLike', found $($roots.Count); pass -PathLike or -ProcessId"
}
$root = $roots[0]

# Every descendant, by ParentProcessId (WebView2's browser process is the app's child;
# its renderers, GPU and utility processes are the browser's children).
$children = @{}
foreach ($p in $all) {
    $key = [int]$p.ParentProcessId
    if (-not $children.ContainsKey($key)) { $children[$key] = @() }
    $children[$key] += $p
}
$webviews = @()
$queue = [System.Collections.Generic.Queue[int]]::new()
$queue.Enqueue([int]$root.ProcessId)
while ($queue.Count) {
    $id = $queue.Dequeue()
    foreach ($c in @($children[$id])) {
        if (-not $c) { continue }
        # A parent id can be reused by a newer process: a child must be younger.
        if ($c.CreationDate -lt $root.CreationDate) { continue }
        if ($c.Name -eq 'msedgewebview2.exe') { $webviews += $c }
        $queue.Enqueue([int]$c.ProcessId)
    }
}

function Snapshot([int[]]$ids) {
    $procs = @($ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    [pscustomobject]@{
        Count   = $procs.Count
        Private = ($procs | Measure-Object PrivateMemorySize64 -Sum).Sum
        WS      = ($procs | Measure-Object WorkingSet64 -Sum).Sum
        Cpu     = ($procs | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
    }
}

$appIds = @([int]$root.ProcessId)
$wvIds = @($webviews | ForEach-Object { [int]$_.ProcessId })
$cores = [Environment]::ProcessorCount
$app0 = Snapshot $appIds
$wv0 = Snapshot $wvIds
$clock = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $Seconds
$elapsed = $clock.Elapsed.TotalMilliseconds
$app1 = Snapshot $appIds
$wv1 = Snapshot $wvIds

function Row($what, $before, $after) {
    $cpu = if ($elapsed) { 100 * ($after.Cpu - $before.Cpu) / ($elapsed * $cores) } else { 0 }
    [pscustomobject]@{
        process       = $what
        count         = $after.Count
        'private MB'  = [math]::Round($after.Private / 1MB, 1)
        'working MB'  = [math]::Round($after.WS / 1MB, 1)
        'CPU %'       = [math]::Round($cpu, 2)
    }
}

Write-Host "$($root.ExecutablePath) (pid $($root.ProcessId)), CPU over $Seconds s on $cores logical cores"
@(
    (Row "$Name.exe" $app0 $app1),
    (Row 'msedgewebview2.exe' $wv0 $wv1)
) | Format-Table -AutoSize
