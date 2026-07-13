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

### Publish a release

1. Bump the app version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` (the workflow checks that they match).
2. Commit and push the version change, then push the release tag: `git tag v1.0.2 && git push origin v1.0.2`.
3. GitHub Actions runs tests, checks the version, builds all platforms, signs updater artifacts, and publishes them to the public `EsanOne-Releases` repository. If any check fails, no release is published.

Update endpoint:

`https://github.com/reoniazx/EsanOne-Releases/releases/latest/download/latest.json`

> **Note:** Source remains private. The public `EsanOne-Releases` repository is the only update channel; do not point the updater at the private source repository.
