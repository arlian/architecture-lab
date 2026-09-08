# Starts the gateway plus all three providers, each in its own PowerShell
# window, from the workspace root. Ctrl-C each window to stop it.
#
# No broker and no Docker needed for this lab — every edge is a plain HTTP call.
#
# Ports: gateway 3010, catalog 3011, partner 3012, archive 3013.
#
# Defaults are chosen so the interesting case happens immediately: partner
# stalls 800ms against the gateway's 500ms budget (so it always times out) and
# archive fails half its searches. Override either when launching by hand:
#   $env:LATENCY_MS=100;  cargo run -p partner-provider
#   $env:FAILURE_RATE=0;  cargo run -p archive-provider

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting catalog-provider on :3011 (fast, well-behaved) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p catalog-provider"

Write-Host "Starting partner-provider on :3012 (slow: 800ms) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p partner-provider"

Write-Host "Starting archive-provider on :3013 (flaky: 50% 503s) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p archive-provider"

# The gateway dials providers lazily, per request, so it does not actually need
# them up first — a provider that is still compiling just shows up as a `failed`
# branch. Pausing anyway so the first curl looks the way the README says it does.
Start-Sleep -Seconds 2

Write-Host "Starting search-gateway on :3010 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p search-gateway"

Write-Host ""
Write-Host "All four launching in separate windows."
Write-Host "  gateway  -> http://localhost:3010/search?q=mug"
Write-Host "  catalog  -> http://localhost:3011/search?q=mug"
Write-Host "  partner  -> http://localhost:3012/search?q=mug  (800ms)"
Write-Host "  archive  -> http://localhost:3013/search?q=mug  (503s half the time)"
Write-Host ""
Write-Host "Run the same query a few times — the answer changes shape, not status code."
