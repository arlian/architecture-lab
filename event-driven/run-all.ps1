# Starts NATS (via Docker) plus all four services, each in its own PowerShell
# window, from the workspace root. Ctrl-C each service window to stop it; the
# NATS container needs `docker stop nats-dev` separately.
#
# Ports: NATS 4222, users 3001, catalog 3002, orders 3003. Notifications has no
# HTTP port at all — it's a pure event consumer, watch its window for log lines.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting NATS on :4222 (Docker) ..."
docker run --rm -d --name nats-dev -p 4222:4222 nats:2-alpine | Out-Null
Start-Sleep -Seconds 1

Write-Host "Starting users-service on :3001 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p users-service"

Write-Host "Starting catalog-service on :3002 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p catalog-service"

Write-Host "Starting notifications-service ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p notifications-service"

# Give NATS and the two producers a moment before Orders starts building its
# read model from their events.
Start-Sleep -Seconds 2

Write-Host "Starting orders-service on :3003 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p orders-service"

Write-Host ""
Write-Host "All services launching in separate windows; NATS running in Docker as 'nats-dev'."
Write-Host "  nats          -> nats://localhost:4222"
Write-Host "  users         -> http://localhost:3001"
Write-Host "  catalog       -> http://localhost:3002"
Write-Host "  orders        -> http://localhost:3003"
Write-Host "  notifications -> (no HTTP; watch its window for log lines)"
Write-Host ""
Write-Host "When done: 'docker stop nats-dev' to remove the NATS container."
