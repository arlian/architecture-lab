# Starts all three services, each in its own PowerShell window, from the
# workspace root. Handy for local exploration. Ctrl-C each window to stop.
#
# Ports: users 3001, catalog 3002, orders 3003. Orders reads USERS_URL /
# CATALOG_URL from the environment; the defaults already point at the ports below.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting users-service on :3001 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p users-service"

Write-Host "Starting catalog-service on :3002 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p catalog-service"

# Give the dependencies a moment to bind before Orders starts calling them.
Start-Sleep -Seconds 2

Write-Host "Starting orders-service on :3003 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p orders-service"

Write-Host ""
Write-Host "All three services launching in separate windows."
Write-Host "  users   -> http://localhost:3001"
Write-Host "  catalog -> http://localhost:3002"
Write-Host "  orders  -> http://localhost:3003"
