# scan-station example setup

Copy this folder to your server, configure, and run.

## Quick start (filesystem only, zero config)

```sh
cp -r example/ ~/scanner
cd ~/scanner
docker compose up -d
```

Scanned PDFs appear in `./scans/`.

## With multiple backends

1. Edit `.env` with your API keys
2. Edit `config.toml` — uncomment the backends you want
3. Run:

```sh
docker compose up -d
```

## Files

| File | Purpose |
|------|---------|
| `docker-compose.yml` | Container config with all mount points and env vars |
| `.env` | Secrets and settings (not committed to git) |
| `config.toml` | Backend configuration (safe to commit — secrets use `${ENV_VAR}`) |
| `gcloud-sa.json` | Google Drive service account key (create if using gdrive backend) |

## Required mounts

| Container path | Host path | Purpose |
|---------------|-----------|---------|
| `/scans` | `./scans` or NFS share | Scanned PDF output |
| `/var/run/dbus` | `/var/run/dbus` | USB scanner communication |

## Optional mounts

| Container path | Host path | Purpose |
|---------------|-----------|---------|
| `/etc/scan-station/config.toml` | `./config.toml` | Multi-backend config (read-only) |
| `/etc/scan-station/gcloud-sa.json` | `./gcloud-sa.json` | Google Drive credentials (read-only) |
