# Compares startup time and RAM: official Riot Client + League UI vs LoLite.
# Requires PowerShell 7 (pwsh). Force-closes every Riot/League process and LoLite first; don't run during a game.
#   pwsh -File scripts/bench.ps1 -Mode full     # official client
#   pwsh -File scripts/bench.ps1 -Mode lolite   # target/release/lolite.exe (cargo build --release first)
param([ValidateSet('full', 'lolite')][string]$Mode = 'lolite', [int]$Timeout = 180, [int]$Settle = 30)

$rcs = 'D:\Riot Games\Riot Client\RiotClientServices.exe'
$lcuLf = 'D:\Riot Games\League of Legends\lockfile'
$lolite = Join-Path $PSScriptRoot '..\target\release\lolite.exe'
$names = 'RiotClientServices', 'Riot Client', 'LeagueClient', 'LeagueClientUx', 'LeagueClientUxRender', 'lolite'

Get-Process -Name $names -EA SilentlyContinue | Stop-Process -Force
Start-Sleep 3

$sw = [Diagnostics.Stopwatch]::StartNew()
if ($Mode -eq 'full') {
  Start-Process $rcs -ArgumentList '--launch-product=league_of_legends', '--launch-patchline=live'
} else {
  Start-Process $lolite
}

# Ready = League client API answers the queue list (what LoLite needs to show "Play").
# A stale lockfile from the killed client just fails until the new one is written.
$ready = $null
while (-not $ready -and $sw.Elapsed.TotalSeconds -lt $Timeout) {
  if (Test-Path $lcuLf) {
    try {
      $l = (Get-Content $lcuLf -EA Stop).Split(':')
      $h = @{ Authorization = 'Basic ' + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("riot:$($l[3])")) }
      Invoke-RestMethod "https://127.0.0.1:$($l[2])/lol-game-queues/v1/queues" -Headers $h -SkipCertificateCheck -TimeoutSec 2 | Out-Null
      $ready = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    } catch {}
  }
  Start-Sleep -Milliseconds 200
}
if (-not $ready) { "not ready after ${Timeout}s (not logged in?)"; exit 1 }
"[$Mode] ready to queue at ${ready}s"

Start-Sleep $Settle
"--- RAM (MB) after ${Settle}s settle:"
$p = Get-Process -Name $names -EA SilentlyContinue
$p | Group-Object Name | ForEach-Object { '{0,-24} {1,6}' -f $_.Name, [int](($_.Group | Measure-Object WorkingSet64 -Sum).Sum / 1MB) }
'{0,-24} {1,6}' -f 'TOTAL', [int](($p | Measure-Object WorkingSet64 -Sum).Sum / 1MB)

if ($Mode -eq 'lolite') {
  $c1 = (Get-Process lolite).TotalProcessorTime.TotalMilliseconds
  Start-Sleep 10
  $c2 = (Get-Process lolite).TotalProcessorTime.TotalMilliseconds
  'lolite idle CPU: {0:N2}% of one core' -f (($c2 - $c1) / 100)
  'lolite.exe size: {0:N1} MB' -f ((Get-Item $lolite).Length / 1MB)
}
