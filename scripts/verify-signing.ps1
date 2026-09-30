$ErrorActionPreference = "Stop"
$files = @("src-tauri/target/release/ubra.exe")
$sidecars = @(Get-ChildItem src-tauri/binaries -File -Filter "ubra-*.exe")
if ($sidecars.Count -lt 2) { throw "Missing staged daemon/CLI sidecars." }
$files += @($sidecars | ForEach-Object { $_.FullName })
$installers = @(Get-ChildItem src-tauri/target/release/bundle -Recurse -File | Where-Object { $_.Extension -in @(".msi", ".exe") })
if ($installers.Count -eq 0) { throw "No Windows installer to verify." }
$files += @($installers | ForEach-Object { $_.FullName })
foreach ($path in $files) {
  $signature = Get-AuthenticodeSignature $path
  if ($signature.Status -ne "Valid") { throw "Invalid or missing Authenticode signature: $path" }
  if ($signature.SignerCertificate.Thumbprint -ne $env:UBRA_SIGNING_THUMBPRINT) {
    throw "Unexpected signing certificate: $path"
  }
}
