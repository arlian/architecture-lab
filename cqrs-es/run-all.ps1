# Starts NATS (via Docker) plus all five services, each in its own PowerShell
# window, from the workspace root. Ctrl-C each service window to stop it; the
# NATS container needs `docker stop nats-dev` separately.
#
# Ports: NATS 4222, users 3001, catalog 3002, orders-command 3003,
# orders-query 3004. Notifications has no HTTP port at all.

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

Write-Host "Starting orders-query-service on :3004 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p orders-query-service"

# Give NATS and the other producers a moment before orders-command starts
# building its own read model of users/products.
Start-Sleep -Seconds 2

Write-Host "Starting orders-command-service on :3003 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p orders-command-service"

Write-Host ""
Write-Host "All services launching in separate windows; NATS running in Docker as 'nats-dev'."
Write-Host "  nats            -> nats://localhost:4222"
Write-Host "  users           -> http://localhost:3001"
Write-Host "  catalog         -> http://localhost:3002"
Write-Host "  orders-command  -> http://localhost:3003  (writes: place/pay/ship/cancel)"
Write-Host "  orders-query    -> http://localhost:3004  (reads: list/get)"
Write-Host "  notifications   -> (no HTTP; watch its window for log lines)"
Write-Host ""
Write-Host "When done: 'docker stop nats-dev' to remove the NATS container."
