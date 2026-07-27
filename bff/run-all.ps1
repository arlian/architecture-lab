# Starts all five services, each in its own PowerShell window, from the
# workspace root. Handy for local exploration. Ctrl-C each window to stop.
#
# Ports: users 3001, catalog 3002, orders 3003, web-bff 3004, mobile-bff 3005.
# orders reads USERS_URL/CATALOG_URL; the BFFs read ORDERS_URL/USERS_URL
# (web-bff also reads CATALOG_URL) — defaults already point at the ports below.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "Starting users-service on :3001 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p users-service"

Write-Host "Starting catalog-service on :3002 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p catalog-service"

# Give users/catalog a moment to bind before orders starts calling them.
Start-Sleep -Seconds 2

Write-Host "Starting orders-service on :3003 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p orders-service"

# Give orders a moment to bind before the BFFs start calling it.
Start-Sleep -Seconds 2

Write-Host "Starting web-bff on :3004 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p web-bff"

Write-Host "Starting mobile-bff on :3005 ..."
Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$root'; cargo run -p mobile-bff"

Write-Host ""
Write-Host "All five services launching in separate windows."
Write-Host "  users      -> http://localhost:3001"
Write-Host "  catalog    -> http://localhost:3002"
Write-Host "  orders     -> http://localhost:3003"
Write-Host "  web-bff    -> http://localhost:3004"
Write-Host "  mobile-bff -> http://localhost:3005"
