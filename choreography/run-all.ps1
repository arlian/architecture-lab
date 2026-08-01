# Starts NATS (via Docker) plus all six services, each in its own PowerShell
# window, from the workspace root. Ctrl-C each service window to stop it; the
# NATS container needs `docker stop nats-dev` separately.
#
# Ports: NATS 4222, users 3001, catalog 3002, orders 3003, inventory 3004,
# payments 3005. Notifications has no HTTP port at all — it's a pure event
# consumer, watch its window for log lines.
#
# Unlike the saga lab's version of this script, **the order below doesn't
# matter**. There is no orchestrator that has to come up last because it
# drives everyone else; every service here only subscribes to subjects and
# reacts. Start them in any order you like.
#
# It does still matter that a service is *running* when a fact it cares about
# is published, though — plain NATS core has no replay, so anything published
# while a subscriber is down is gone for that subscriber forever. That's why
# the script still pauses before telling you it's ready: give everyone a
# moment to subscribe before you place an order.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting NATS on :4222 (Docker) ..."
docker run --rm -d --name nats-dev -p 4222:4222 nats:2-alpine | Out-Null
Start-Sleep -Seconds 1

Write-Host "Starting users-service on :3001 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p users-service"

Write-Host "Starting catalog-service on :3002 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p catalog-service"

Write-Host "Starting inventory-service on :3004 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p inventory-service"

Write-Host "Starting payments-service on :3005 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p payments-service"

Write-Host "Starting notifications-service ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p notifications-service"

Write-Host "Starting orders-service on :3003 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p orders-service"

# Not a dependency ordering — just time for six `cargo run`s to compile, boot
# and subscribe before you start publishing at them.
Start-Sleep -Seconds 2

Write-Host ""
Write-Host "All services launching in separate windows; NATS running in Docker as 'nats-dev'."
Write-Host "  nats          -> nats://localhost:4222"
Write-Host "  users         -> http://localhost:3001"
Write-Host "  catalog       -> http://localhost:3002"
Write-Host "  orders        -> http://localhost:3003"
Write-Host "  inventory     -> http://localhost:3004"
Write-Host "  payments      -> http://localhost:3005"
Write-Host "  notifications -> (no HTTP; watch its window for log lines)"
Write-Host ""
Write-Host "When done: 'docker stop nats-dev' to remove the NATS container."
