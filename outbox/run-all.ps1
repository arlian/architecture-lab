# Starts the ledger and both orders-service modes, each in its own window.
#
# Ports: naive 3030, outbox 3031, ledger 3040. Then:
#
#   cargo run -p audit-runner
#
# Knob: $env:CRASH_RATE=0.3 before launching makes every failure louder.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting ledger-service on :3040 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p ledger-service"

Write-Host "Starting orders-service (naive) on :3030 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; `$env:MODE='naive'; `$env:PORT=3030; cargo run -p orders-service"

Write-Host "Starting orders-service (outbox) on :3031 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; `$env:MODE='outbox'; `$env:PORT=3031; cargo run -p orders-service"

Write-Host ""
Write-Host "Once all three are listening:"
Write-Host "  cargo run -p audit-runner"
