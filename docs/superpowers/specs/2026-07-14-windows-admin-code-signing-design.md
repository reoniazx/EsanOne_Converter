# EsanOne Converter: Windows Admin Installer and Code Signing

## Goal

Make the Windows installer install machine-wide with Administrator approval and
ship release artifacts with a real Authenticode signature so Windows can show a
known Publisher instead of `Unknown publisher`.

The scope is the installer and release pipeline only. The installed app itself
must continue to run as a normal user process.

## Current state

- Tauri 2 builds Windows NSIS and MSI artifacts through `bundle.targets: "all"`.
- The reported artifact is the NSIS `x64-setup.exe` installer.
- The NSIS installer currently uses Tauri's default per-user install mode.
- The release workflow already signs Tauri updater artifacts with the existing
  updater key, but it does not Authenticode-sign Windows executables/installers.
- No Windows certificate or private signing material may be committed to the
  repository.

## Design

### 1. Windows installer mode

Add `src-tauri/tauri.windows.conf.json` as the platform-specific Tauri config.
It will merge with the existing `tauri.conf.json` and set:

- `bundle.publisher` to `EsanOne`.
- `bundle.windows.nsis.installMode` to `perMachine`.

The existing `targets: "all"` setting remains unchanged. `perMachine` applies to
the NSIS setup executable shown in the user's report; the application binary
does not receive an always-run-as-admin manifest.

### 2. Windows Authenticode signing

The Windows matrix job in `.github/workflows/release.yml` will import a
certificate only on `windows-latest`:

1. Read the Base64-encoded `.pfx` from `WINDOWS_CERTIFICATE`.
2. Decode it into a runner-local temporary certificate file.
3. Import it into `Cert:\CurrentUser\My` using
   `WINDOWS_CERTIFICATE_PASSWORD`.
4. Read the certificate thumbprint and write a temporary Tauri signing config
   containing the thumbprint, SHA-256 digest algorithm, and the timestamp URL
   supplied by the certificate issuer.
5. Run the existing Tauri release action with that temporary config.
6. Remove temporary certificate files after the build step.

The workflow will fail on the Windows build if the certificate secrets are
missing or the certificate cannot be imported. It must not silently fall back
to an unsigned installer.

The two new GitHub Secrets are:

| Secret | Contents |
| --- | --- |
| `WINDOWS_CERTIFICATE` | Base64-encoded `.pfx` containing the private key |
| `WINDOWS_CERTIFICATE_PASSWORD` | Export password for that `.pfx` |

These secrets are separate from `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the latter sign updater payloads and are
not Windows Authenticode credentials.

The certificate should be an actual Windows code-signing certificate. An OV
certificate can continue to show a SmartScreen warning until reputation builds;
EV or a trusted signing service can establish reputation faster. Code signing
cannot guarantee that the first download on every machine will be warning-free.

### 3. Documentation and verification

Update `README.md` with:

- How to create the Base64 `.pfx` secret values.
- The exact GitHub Secret names.
- The fact that the certificate must be a code-signing certificate, not an SSL
  certificate.
- Local/release verification commands and the SmartScreen reputation caveat.

Verification will cover:

- JSON/config validation and a Tauri debug/no-bundle build.
- Existing frontend and Rust test suites.
- A signed Windows release check using `signtool verify /pa` against the
  generated installer and application executable.
- Manual confirmation that the UAC dialog shows the configured Publisher.

## Error handling and security

- Certificate material is present only in the ephemeral GitHub runner and is
  never written to tracked files.
- Temporary files are removed after signing; runner teardown is the final
  containment boundary if the build fails.
- Missing secrets, invalid Base64, a wrong password, missing private key, or a
  failed signature must fail the Windows release job.
- The existing updater signing key and updater endpoint remain unchanged.
- macOS and Linux release matrix entries do not import Windows material and do
  not receive Windows-specific configuration.

## Acceptance criteria

1. Running the generated NSIS setup executable requests Administrator approval.
2. The app installs machine-wide without making the running app itself require
   Administrator privileges.
3. A release built with the configured secrets contains a valid Authenticode
   signature on the Windows installer and app executable.
4. Windows file properties and the UAC prompt show `EsanOne` (or the legal
   certificate subject chosen for the purchased certificate) rather than
   `Unknown publisher`.
5. The workflow fails rather than publishing an unsigned Windows artifact when
   signing setup is incomplete.
6. Existing updater signatures and non-Windows release artifacts continue to
   work.

## References

- Tauri Windows installer install modes:
  https://v2.tauri.app/distribute/windows-installer/
- Tauri Windows code signing:
  https://v2.tauri.app/distribute/sign/windows/
