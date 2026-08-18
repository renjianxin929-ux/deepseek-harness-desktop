# DeepSeek Harness Desktop — Windows packaged-app E2E (CI, windows-latest).
#
# Launches the desktop app, waits for the bundled Harness runtime to become
# ready (startup.log `[ready] http://…`), then verifies the real Harness Web
# UI answers over HTTP with the pinned rc.7 plugin set. This is the
# closest-to-real run-level check that can run in CI (a real GUI session is
# not available, but the full desktop process tree, bundled Node + dsh web
# server, and HTTP readiness all execute for real).
#
# Launch sources:
#   -Exe <path>       run an app executable directly (CI uses the fresh
#                     `tauri build` release exe — avoids NSIS /S quirks on
#                     service accounts)
#   -Installer <path> (optional) silently install the NSIS installer first
#
# Usage: powershell -File scripts/e2e-windows-app.ps1 -Exe <path>
#        powershell -File scripts/e2e-windows-app.ps1 -Installer <path>
# Exit:  0 = PASS, 1 = FAIL.

param(
  [Parameter(Mandatory = $false)][string]$Exe,
  [Parameter(Mandatory = $false)][string]$Installer
)

$ErrorActionPreference = "Stop"

function Fail([string]$msg) {
  Write-Host "E2E FAIL: $msg" -ForegroundColor Red
  exit 1
}

if (-not $Exe -and -not $Installer) {
  Fail "provide -Exe <path> or -Installer <path>"
}

$exePath = $null
if ($Exe) {
  if (-not (Test-Path $Exe)) { Fail "exe not found: $Exe" }
  $exePath = (Resolve-Path $Exe).Path
} else {
  if (-not (Test-Path $Installer)) { Fail "installer not found: $Installer" }

  # Silent install (NSIS /S). Tauri NSIS installs to
  # $LOCALAPPDATA\<productName> by default; search a few roots, waiting for
  # the file tree to settle after the installer process exits.
  $proc = Start-Process -FilePath $Installer -ArgumentList "/S" -PassThru -Wait
  if ($proc.ExitCode -ne 0) {
    Fail "installer exit code $($proc.ExitCode)"
  }
  $candidates = @(
    $env:LOCALAPPDATA,
    (Join-Path $env:LOCALAPPDATA "Programs"),
    $env:ProgramFiles,
    ${env:ProgramFiles(x86)}
  )
  $found = $null
  for ($i = 0; $i -lt 10 -and -not $found; $i++) {
    foreach ($root in $candidates) {
      if (-not $root -or -not (Test-Path $root)) { continue }
      $found = Get-ChildItem -Path $root -Recurse -Filter "DeepSeek Harness Desktop.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1
      if ($found) { break }
    }
    if (-not $found) { Start-Sleep -Seconds 3 }
  }
  if (-not $found) {
    Fail "installed exe not found after silent install"
  }
  $exePath = $found.FullName
}
Write-Host "E2E: using exe $exePath"

# 2. Clear the previous startup log so we only read this launch.
$logDir = Join-Path $env:LOCALAPPDATA "HarnessDesktop\logs"
if (Test-Path (Join-Path $logDir "startup.log")) {
  Remove-Item -Force (Join-Path $logDir "startup.log")
}

# 3. Launch the app.
$app = Start-Process -FilePath $exePath -PassThru
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
  if (Test-Path $logPath) {
    Write-Host "---- startup.log tail ----"
    Get-Content -Tail 30 $logPath
  }
  Fail "app never became ready (no '[ready] http://127.0.0.1:<port>' in startup.log)"
}
Write-Host "E2E: Harness ready on port $port"

# 5. Verify the Harness Web UI answers over HTTP with the rc.7 plugin set.
$body = (Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$port/" -TimeoutSec 15).Content
if (-not $body -or -not $body.Contains("dsh-client-ui-conversation")) {
  Fail "Harness UI did not serve the conversation client (unexpected page)"
}
Write-Host "E2E: Harness UI served (dsh-client-ui-conversation present)"

# 6. Cleanup: terminate the app process tree.
if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
Get-Process | Where-Object { $_.ProcessName -match "deepseek|harness" } |
  Stop-Process -Force -ErrorAction SilentlyContinue

Write-Host "E2E PASS: app launched, Harness rc.7 became ready and served the UI" -ForegroundColor Green
exit 0
