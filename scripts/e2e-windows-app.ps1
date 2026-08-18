# DeepSeek Harness Desktop — Windows packaged-app E2E (CI, windows-latest).
#
# Installs the NSIS installer silently, launches the packaged app, waits for
# the bundled Harness runtime to become ready (startup.log `[ready] http://…`),
# then verifies the real Harness Web UI answers over HTTP with the pinned
# rc.7 plugin set. This is the closest-to-real run-level check that can run in
# CI (a real GUI session is not available, but the full desktop process tree,
# bundled Node + dsh web server, and HTTP readiness all execute for real).
#
# Usage:  powershell -File scripts/e2e-windows-app.ps1 -Installer <path>
# Exit:   0 = PASS, 1 = FAIL.

param(
  [Parameter(Mandatory = $true)][string]$Installer
)

$ErrorActionPreference = "Stop"

function Fail([string]$msg) {
  Write-Host "E2E FAIL: $msg" -ForegroundColor Red
  exit 1
}

if (-not (Test-Path $Installer)) {
  Fail "installer not found: $Installer"
}

# 1. Install silently (NSIS /S). NSIS `/D=` must be the last argument and is
#    quoted-path sensitive; instead of fighting it we install to the
#    installer's default location and then locate the exe under LOCALAPPDATA.
$proc = Start-Process -FilePath $Installer -ArgumentList "/S" -PassThru -Wait
if ($proc.ExitCode -ne 0) {
  Fail "installer exit code $($proc.ExitCode)"
}
$exe = Get-ChildItem -Path $env:LOCALAPPDATA -Recurse -Filter "DeepSeek Harness Desktop.exe" -ErrorAction SilentlyContinue |
  Select-Object -First 1
if (-not $exe) {
  # Also try the per-user Programs directory (NSIS default for per-user installs).
  $exe = Get-ChildItem -Path (Join-Path $env:LOCALAPPDATA "Programs") -Recurse -Filter "DeepSeek Harness Desktop.exe" -ErrorAction SilentlyContinue |
    Select-Object -First 1
}
if (-not $exe) {
  Fail "installed exe not found under LOCALAPPDATA after silent install"
}
$exe = $exe.FullName
Write-Host "E2E: installed to $exe"

# 2. Clear the previous startup log so we only read this launch.
$logDir = Join-Path $env:LOCALAPPDATA "HarnessDesktop\logs"
if (Test-Path (Join-Path $logDir "startup.log")) {
  Remove-Item -Force (Join-Path $logDir "startup.log")
}

# 3. Launch the packaged app.
$app = Start-Process -FilePath $exe -PassThru
Write-Host "E2E: launched app pid=$($app.Id)"

# 4. Wait up to 120s for `[ready] http://127.0.0.1:<port>` in startup.log.
$logPath = Join-Path $logDir "startup.log"
$deadline = (Get-Date).AddSeconds(120)
$port = $null
while ((Get-Date) -lt $deadline) {
  if (Test-Path $logPath) {
    $log = Get-Content -Raw -Path $logPath -ErrorAction SilentlyContinue
    if ($log) {
      $m = [regex]::Match($log, "\[ready\] http://127\.0\.0\.1:(\d+)")
      if ($m.Success) {
        $port = $m.Groups[1].Value
        break
      }
    }
  }
  Start-Sleep -Seconds 2
}
if (-not $port) {
  # Diagnostics: dump the tail of the log + running processes before failing.
  if (Test-Path $logPath) {
    Write-Host "---- startup.log tail ----"
    Get-Content -Tail 30 $logPath
  }
  Write-Host "---- harness/node processes ----"
  Get-Process | Where-Object { $_.ProcessName -match "deepseek|harness|node" } |
    Select-Object ProcessName, Id | Format-Table | Out-String | Write-Host
  Fail "app never became ready (no '[ready] http://127.0.0.1:<port>' in startup.log)"
}
Write-Host "E2E: Harness ready on port $port"

# 5. Verify the Harness Web UI answers over HTTP with the pinned rc.7 plugin
#    set (index.html must reference the real conversation/theme client).
$body = (Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$port/" -TimeoutSec 15).Content
if (-not $body -or -not $body.Contains("dsh-client-ui-conversation")) {
  Fail "Harness UI did not serve the conversation client (unexpected page)"
}
Write-Host "E2E: Harness UI served (dsh-client-ui-conversation present)"

# 6. Cleanup: terminate the app process tree.
if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
Get-Process | Where-Object { $_.ProcessName -match "deepseek|harness" } |
  Stop-Process -Force -ErrorAction SilentlyContinue

Write-Host "E2E PASS: packaged app installed, launched, Harness rc.7 became ready and served the UI" -ForegroundColor Green
exit 0
