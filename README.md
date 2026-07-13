# EsanOne Convert

Local stem separation for Mac, Windows, and Linux — paste a YouTube link or drop an audio file, see **Key** / **BPM**, then separate with:

1. **Vocals** — MelBandRoFormer (`melbandroformer_vocals`) → Vocals + Instrumental  
2. **Instruments 4** — HT-Demucs (`htdemucs_ft`) → Drums, Bass, Other, Vocals  
3. **Instruments 6** — HT-Demucs (`htdemucs_6s`) → + Guitar, Piano  

Zero-config: the app installs Python, ffmpeg, PyTorch (CPU or CUDA), and model weights when needed. Full Thai / English UI. Theme is near-black `#121212` with white accents.

## Stack

- Tauri 2 + React + TypeScript + Motion  
- Rust bridge ports the Esan One Studio stem pipeline  
- Managed Python backends (Demucs + MelBandRoFormer wrapper)

## Develop

```bash
npm install
npm run tauri dev
```

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

## Right-click menus

Use **Enable right-click menus** in the app footer to register cascading OS actions (Windows registry / macOS Service / Linux `.desktop` actions).

CLI form:

```text
EsanOne Convert.exe separate --model melbandroformer_vocals --path "C:\song.wav"
```

## Dochord song search (optional)

หลังดาวน์โหลดเพลงจาก YouTube แอปสามารถค้นหาเพลงด้วย Google และแสดงเฉพาะลิงก์จาก Dochord ได้ โดยไม่ดึงหรือเก็บเนื้อเพลงและคอร์ดไว้ในแอป

ตั้งค่าในไฟล์ `.env` (คัดลอกจาก `.env.example`) แล้วเริ่มแอปใหม่:

```text
VITE_GOOGLE_CUSTOM_SEARCH_API_KEY=your-google-api-key
VITE_GOOGLE_CUSTOM_SEARCH_CX=your-programmable-search-engine-id
```

สร้าง Google Programmable Search Engine ให้ค้นหาเฉพาะ `dochord.com` และเปิดใช้ Custom Search JSON API ใน Google Cloud จากนั้นจำกัด API key ให้ใช้กับ API นี้และตั้ง quota ตามต้องการ ห้าม commit ค่า API key จริงลง source repository

ถ้ายังไม่ตั้งค่า แอปจะแสดงปุ่ม **ค้นหาเพลงใน Google** เพื่อเปิดผลค้นหา `site:dochord.com` ในเบราว์เซอร์แทน

## In-app updates

Installed builds can update themselves from GitHub Releases — open the app footer and press **Update** (or **Check for updates**). The app downloads the signed package, installs it, and relaunches. No uninstall required.

### One-time signing setup

A signing keypair already exists on the machine that generated this project (`%USERPROFILE%\.tauri\esanone-stem.key`). The filename is retained so existing signed updates remain compatible. Add these GitHub Actions secrets:

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Full contents of `esanone-stem.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password used when generating the key |
| `ESANONE_CONVERT_RELEASES_TOKEN` | GitHub token with `Contents: write` on `reoniazx/EsanOne-Releases` |

Use a fine-grained token scoped only to `reoniazx/EsanOne-Releases` with `Contents: Read and write`; do not reuse a broad personal token.

The public key is already in `src-tauri/tauri.conf.json`. If you regenerate keys, replace `plugins.updater.pubkey` and store the new private key as the secret above.

### Windows installer signing

The Windows NSIS installer is configured for a per-machine install, so Windows asks for Administrator approval and installs the app for all users. The app itself continues to run as a normal user process.

To replace the red **Unknown publisher** warning, the Windows release must be Authenticode-signed with a real Windows code-signing certificate. SSL certificates do not work. Add these GitHub Actions secrets:

| Secret | Value |
| --- | --- |
| `WINDOWS_CERTIFICATE` | One-line Base64 content of a `.pfx` certificate containing its private key |
| `WINDOWS_CERTIFICATE_PASSWORD` | Export password for the `.pfx` file |

Also add this GitHub Actions repository variable:

| Variable | Value |
| --- | --- |
| `WINDOWS_TIMESTAMP_URL` | RFC 3161 timestamp URL recommended by the certificate provider |

On PowerShell, create the Base64 secret value without committing the certificate:

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes('.\certificate.pfx')) | Set-Clipboard
```

Paste the clipboard value into `WINDOWS_CERTIFICATE`. Never commit the `.pfx`, password, or Base64 value. The release job fails when signing is not configured, so it cannot silently publish an unsigned Windows installer.

OV certificates can still show a SmartScreen warning until reputation builds. EV certificates or trusted signing services can establish reputation faster, but no certificate type guarantees that the first download is warning-free on every machine.

To verify a generated installer locally on Windows:

```powershell
signtool verify /pa /all /v .\EsanOne.Converter_*_x64-setup.exe
```

This verifies Authenticode integrity and certificate identity. It does not guarantee immediate SmartScreen reputation. See the official [Tauri Windows installer guide](https://v2.tauri.app/distribute/windows-installer/) and [Tauri Windows code-signing guide](https://v2.tauri.app/distribute/sign/windows/) for certificate details.

### Publish a release

1. Bump the app version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` (the workflow checks that they match).
2. Commit and push the version change, then push the release tag: `git tag v1.0.1 && git push origin v1.0.1`.
3. GitHub Actions runs tests, checks the version, builds all platforms, signs updater artifacts, and publishes them to the public `EsanOne-Releases` repository. If any check fails, no release is published.

Update endpoint:

`https://github.com/reoniazx/EsanOne-Releases/releases/latest/download/latest.json`

> **Note:** Source remains private. The public `EsanOne-Releases` repository is the only update channel; do not point the updater at the private source repository.
