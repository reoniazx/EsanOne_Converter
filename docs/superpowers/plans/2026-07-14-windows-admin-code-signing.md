# Windows Admin Installer and Code Signing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (\`- [ ]\`) syntax for tracking.

**Goal:** Make the Windows NSIS installer machine-wide/Admin-approved and make the Windows release job fail-safe Authenticode-sign its executable and installer.

**Architecture:** Add a Tauri platform-specific Windows config for NSIS \`perMachine\` installation. The Windows GitHub Actions matrix entry imports a Base64 \`.pfx\` into the ephemeral runner, generates a temporary signing overlay with the certificate thumbprint and timestamp URL, builds with that overlay, verifies signatures, and removes temporary files. Existing updater signing and non-Windows matrix entries remain unchanged.

**Tech Stack:** Tauri 2 config, NSIS, GitHub Actions, PowerShell, Windows certificate store, Authenticode \`signtool\`, React/Vite frontend tests, Rust/Cargo tests.

## Global Constraints

- The installed app itself must continue to run as a normal user process.
- Keep \`bundle.targets: "all"\` and the existing updater identifier, endpoint, public key, and updater signing secrets unchanged.
- Preserve the user's existing uncommitted \`bundle.publisher: "Reoniazx"\` change in \`src-tauri/tauri.conf.json\`; do not stage or rewrite that file.
- Never commit a certificate, private key, password, thumbprint, or generated signing config.
- Missing Windows signing secrets or an invalid certificate must fail the Windows release job instead of publishing an unsigned artifact.
- Use SHA-256 Authenticode signing and a timestamp URL supplied by the certificate provider through the \`WINDOWS_TIMESTAMP_URL\` GitHub Actions repository variable.
- Run focused checks after each task and the complete frontend/Rust/build checks before claiming completion.

## File Map

- Create: \`src-tauri/tauri.windows.conf.json\` — Windows-only NSIS machine-wide install mode.
- Modify: \`.github/workflows/release.yml\` — certificate import, signing overlay, cleanup, and signature verification.
- Modify: \`README.md\` — certificate and release setup instructions.
- Create: \`docs/superpowers/plans/2026-07-14-windows-admin-code-signing.md\` — this plan.
- Preserve without staging: \`src-tauri/tauri.conf.json\` — existing user change adding \`bundle.publisher: "Reoniazx"\`.

---

### Task 1: Add Windows machine-wide installer configuration

**Files:**
- Create: \`src-tauri/tauri.windows.conf.json\`
- Test: JSON assertion and Tauri debug/no-bundle build

**Interfaces:**
- Consumes: Existing \`src-tauri/tauri.conf.json\` and Tauri's automatic platform-specific config merge.
- Produces: Windows-only \`bundle.windows.nsis.installMode = "perMachine"\`.

- [ ] **Step 1: Create the platform-specific config**

Create \`src-tauri/tauri.windows.conf.json\`:

\`\`\`json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "windows": {
      "nsis": {
        "installMode": "perMachine"
      }
    }
  }
}
\`\`\`

- [ ] **Step 2: Validate the config fields**

Run:

\`\`\`powershell
node -e "const c=require('./src-tauri/tauri.windows.conf.json'); if(c.bundle.windows.nsis.installMode!=='perMachine') throw new Error('NSIS install mode is not perMachine'); console.log('Windows NSIS install mode: perMachine')"
\`\`\`

Expected: exit code 0 and output containing \`Windows NSIS install mode: perMachine\`.

- [ ] **Step 3: Verify Tauri accepts the platform config**

Run:

\`\`\`powershell
npm run tauri build -- --debug --no-bundle
\`\`\`

Expected: exit code 0 and \`src-tauri/target/debug/esanone-convert.exe\` exists.

- [ ] **Step 4: Commit only this new file**

\`\`\`powershell
git add src-tauri/tauri.windows.conf.json
git commit -m "build: install Windows app per machine"
\`\`\`

### Task 2: Add fail-safe Windows certificate import and signing

**Files:**
- Modify: \`.github/workflows/release.yml\` in the build matrix and build steps
- Test: diff check and a Windows GitHub Actions release run after secrets are configured

**Interfaces:**
- Consumes: \`WINDOWS_CERTIFICATE\`, \`WINDOWS_CERTIFICATE_PASSWORD\`, and \`WINDOWS_TIMESTAMP_URL\`; existing updater signing secrets remain unchanged.
- Produces: Ephemeral \`certificate/certificate.pfx\` and \`src-tauri/tauri.windows.signing.conf.json\`, plus verified signed release artifacts.

- [ ] **Step 1: Set the Windows matrix build argument**

Change only the Windows matrix entry to:

\`\`\`yaml
args: "--config src-tauri/tauri.windows.signing.conf.json"
\`\`\`

Leave macOS and Ubuntu arguments unchanged.

- [ ] **Step 2: Import the certificate and generate the signing overlay**

Insert this before the existing Tauri action:

\`\`\`yaml
      - name: Import Windows signing certificate
        if: matrix.platform == 'windows-latest'
        shell: pwsh
        env:
          WINDOWS_CERTIFICATE: \${{ secrets.WINDOWS_CERTIFICATE }}
          WINDOWS_CERTIFICATE_PASSWORD: \${{ secrets.WINDOWS_CERTIFICATE_PASSWORD }}
          WINDOWS_TIMESTAMP_URL: \${{ vars.WINDOWS_TIMESTAMP_URL }}
        run: |
          $ErrorActionPreference = 'Stop'
          if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE)) {
            throw 'WINDOWS_CERTIFICATE GitHub Secret is required for Windows releases.'
          }
          if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE_PASSWORD)) {
            throw 'WINDOWS_CERTIFICATE_PASSWORD GitHub Secret is required for Windows releases.'
          }
          if ([string]::IsNullOrWhiteSpace($env:WINDOWS_TIMESTAMP_URL)) {
            throw 'WINDOWS_TIMESTAMP_URL GitHub repository variable is required for Windows releases.'
          }

          $certificateDir = Join-Path $env:GITHUB_WORKSPACE 'certificate'
          New-Item -ItemType Directory -Force -Path $certificateDir | Out-Null
          $pfxPath = Join-Path $certificateDir 'certificate.pfx'
          $signingConfigPath = Join-Path $env:GITHUB_WORKSPACE 'src-tauri\tauri.windows.signing.conf.json'

          try {
            $encoded = ($env:WINDOWS_CERTIFICATE -replace '\s', '')
            [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($encoded))
            $password = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
            $certificate = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $password
            if ($null -eq $certificate -or [string]::IsNullOrWhiteSpace($certificate.Thumbprint)) {
              throw 'The Windows certificate did not import with a usable thumbprint.'
            }

            $signingConfig = [ordered]@{
              bundle = [ordered]@{
                windows = [ordered]@{
                  certificateThumbprint = $certificate.Thumbprint
                  digestAlgorithm = 'sha256'
                  timestampUrl = $env:WINDOWS_TIMESTAMP_URL
                }
              }
            }
            $signingConfig | ConvertTo-Json -Depth 5 | Set-Content -Path $signingConfigPath -Encoding utf8
          } catch {
            Remove-Item -Force -ErrorAction SilentlyContinue $pfxPath, $signingConfigPath
            throw
          }
\`\`\`

The step must not log certificate, password, thumbprint, or generated JSON contents.

- [ ] **Step 3: Verify signatures and always clean up**

Immediately after the Tauri action add:

\`\`\`yaml
      - name: Verify Windows Authenticode signatures
        if: matrix.platform == 'windows-latest'
        shell: pwsh
        run: |
          $ErrorActionPreference = 'Stop'
          $signtool = Get-ChildItem -Path "$env:ProgramFiles(x86)\Windows Kits\10\bin" -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
          if ($null -eq $signtool) {
            throw 'signtool.exe was not found on the Windows runner.'
          }

          $installer = Get-ChildItem -Path 'src-tauri\target\release\bundle\nsis' -Filter '*-setup.exe' -File |
            Select-Object -First 1
          $application = Get-Item 'src-tauri\target\release\esanone-convert.exe' -ErrorAction SilentlyContinue
          if ($null -eq $installer -or $null -eq $application) {
            throw 'Expected signed NSIS installer or application executable was not generated.'
          }

          foreach ($artifact in @($installer, $application)) {
            & $signtool.FullName verify /pa /all /v $artifact.FullName
            if ($LASTEXITCODE -ne 0) {
              throw "Authenticode verification failed for $($artifact.FullName)."
            }
          }

      - name: Remove Windows signing material
        if: always() && matrix.platform == 'windows-latest'
        shell: pwsh
        run: |
          Remove-Item -Recurse -Force -ErrorAction SilentlyContinue 'certificate'
          Remove-Item -Force -ErrorAction SilentlyContinue 'src-tauri\tauri.windows.signing.conf.json'
\`\`\`

Verification must run before cleanup and fail the job if either artifact is unsigned or invalid.

- [ ] **Step 4: Check workflow whitespace**

Run:

\`\`\`powershell
git diff --check -- .github/workflows/release.yml
\`\`\`

Expected: no output and exit code 0.

- [ ] **Step 5: Commit only the workflow change**

\`\`\`powershell
git add .github/workflows/release.yml
git commit -m "ci: sign Windows release artifacts"
\`\`\`

### Task 3: Document certificate and release setup

**Files:**
- Modify: \`README.md\` in the existing release/signing documentation
- Test: Markdown diff check and text inspection

**Interfaces:**
- Consumes: Workflow names \`WINDOWS_CERTIFICATE\`, \`WINDOWS_CERTIFICATE_PASSWORD\`, and \`WINDOWS_TIMESTAMP_URL\`.
- Produces: Reproducible setup instructions without exposing signing material.

- [ ] **Step 1: Add Windows signing setup**

Add a section stating:

- The installer is configured for per-machine installation and asks for Administrator approval.
- A real Windows code-signing certificate is required; SSL certificates do not work.
- \`WINDOWS_CERTIFICATE\` is one-line Base64 of the \`.pfx\` containing the private key.
- \`WINDOWS_CERTIFICATE_PASSWORD\` is the \`.pfx\` export password.
- \`WINDOWS_TIMESTAMP_URL\` is a repository variable containing the provider-recommended RFC 3161 timestamp URL.
- This PowerShell command creates the secret value without committing the certificate:

\`\`\`powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes('.\certificate.pfx')) | Set-Clipboard
\`\`\`

- Never commit the \`.pfx\`, password, or Base64 value.
- The release fails instead of silently publishing an unsigned Windows installer.
- OV SmartScreen reputation may take time; EV or trusted signing service can establish reputation faster.

Include links to the official Tauri Windows installer and Windows code-signing guides.

- [ ] **Step 2: Add verification guidance**

Document:

\`\`\`powershell
signtool verify /pa /all /v .\EsanOne.Converter_*_x64-setup.exe
\`\`\`

Explain that this proves Authenticode integrity/certificate identity, not immediate SmartScreen reputation on every machine.

- [ ] **Step 3: Check and commit documentation**

Run:

\`\`\`powershell
git diff --check -- README.md
git add README.md
git commit -m "docs: document Windows installer signing"
\`\`\`

Expected: diff check passes and only README is committed.

### Task 4: Run the complete verification gate

**Files:**
- Inspect: \`src-tauri/tauri.windows.conf.json\`, \`.github/workflows/release.yml\`, \`README.md\`
- Preserve without staging: \`src-tauri/tauri.conf.json\`

- [ ] **Step 1: Run frontend tests**

\`\`\`powershell
npm test -- --run
\`\`\`

Expected: exit code 0.

- [ ] **Step 2: Run frontend build**

\`\`\`powershell
npm run build
\`\`\`

Expected: exit code 0.

- [ ] **Step 3: Run Rust tests**

\`\`\`powershell
cargo test --manifest-path src-tauri/Cargo.toml
\`\`\`

Expected: exit code 0.

- [ ] **Step 4: Run Tauri debug/no-bundle build**

\`\`\`powershell
npm run tauri build -- --debug --no-bundle
\`\`\`

Expected: exit code 0 and \`src-tauri/target/debug/esanone-convert.exe\` exists.

- [ ] **Step 5: Confirm intended changes and preserve the user edit**

\`\`\`powershell
git status --short
git diff -- src-tauri/tauri.conf.json
\`\`\`

Expected: the existing \`publisher: "Reoniazx"\` diff remains unstaged and unchanged; implementation commits contain only the new Windows config, workflow, and README edits.

- [ ] **Step 6: Report external setup**

After local verification, report that the owner must add the two GitHub Secrets and repository variable before pushing a release tag. A Windows release run with those values is the final proof of signed artifacts; local unsigned builds cannot prove SmartScreen behavior.

