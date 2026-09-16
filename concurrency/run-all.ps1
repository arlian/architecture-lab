# Starts all four strategies, each in its own PowerShell window, from the
# workspace root. Ctrl-C each window to stop it.
#
# No broker, no database, no Docker — four small HTTP services and a CLI.
#
# Ports: naive 3020, optimistic 3021, pessimistic 3022, actor 3023.
#
# Once they are up, the whole lab is one command:
#
#   cargo run -p race-runner -- --target all
#
# Defaults are chosen so the interesting case happens on the first run: a 5ms
# think window is wide enough that naive-service oversells every time. Override
# any knob when launching by hand:
#   $env:THINK_MS=50;         cargo run -p naive-service
#   $env:RETRIES=0;           cargo run -p optimistic-service
#   $env:LOCK_SCOPE="global"; cargo run -p pessimistic-service
#   $env:MAILBOX=4;           cargo run -p actor-service

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting naive-service on :3020 (oversells, on purpose) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p naive-service"

Write-Host "Starting optimistic-service on :3021 (version check + retry) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p optimistic-service"

Write-Host "Starting pessimistic-service on :3022 (per-key locks) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p pessimistic-service"

Write-Host "Starting actor-service on :3023 (single writer, no locks) ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p actor-service"

# The runner seeds before it attacks, so it needs all four actually listening.
# First build can take a while; give them a moment.
Start-Sleep -Seconds 5

Write-Host ""
Write-Host "All four launching in separate windows."
Write-Host "  naive        -> http://localhost:3020"
Write-Host "  optimistic   -> http://localhost:3021"
Write-Host "  pessimistic  -> http://localhost:3022"
Write-Host "  actor        -> http://localhost:3023"
Write-Host ""
Write-Host "Then run the whole lab at once:"
Write-Host "  cargo run -p race-runner -- --target all"
