$ErrorActionPreference = "Stop"
foreach ($name in @("WINDOWS_CERTIFICATE", "WINDOWS_CERTIFICATE_PASSWORD")) {
  if (-not [Environment]::GetEnvironmentVariable($name)) {
    throw "Release signing requires $name; configure it in repository Actions secrets."
  }
}
$certificatePath = Join-Path $env:RUNNER_TEMP "ubra-signing.pfx"
try {
  [IO.File]::WriteAllBytes($certificatePath, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
  $password = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
  $certs = @(Import-PfxCertificate -FilePath $certificatePath -CertStoreLocation Cert:\CurrentUser\My -Password $password)
  $signers = @($certs | Where-Object { $_.HasPrivateKey })
  if ($signers.Count -ne 1) { throw "The PFX must contain exactly one signing certificate with a private key." }
  $cert = $signers[0]
  if ($cert.NotAfter -le (Get-Date)) { throw "The imported certificate has expired." }
  $configPath = Join-Path $env:RUNNER_TEMP "ubra-signing.json"
  @{ bundle = @{ windows = @{
    certificateThumbprint = $cert.Thumbprint
    digestAlgorithm = "sha256"
    timestampUrl = "http://timestamp.digicert.com"
    tsp = $true
  } } } | ConvertTo-Json -Depth 5 | Set-Content $configPath -Encoding utf8
  "UBRA_SIGNING_CONFIG=$configPath" | Out-File $env:GITHUB_ENV -Append -Encoding utf8
  "UBRA_SIGNING_THUMBPRINT=$($cert.Thumbprint)" | Out-File $env:GITHUB_ENV -Append -Encoding utf8
  Write-Output "Imported Windows signing certificate into the release runner's user store."
} finally {
  Remove-Item $certificatePath -Force -ErrorAction SilentlyContinue
}
