# Distribution Setup

This file lists the **manual steps** required to get TuckNotes shipping for sale via Polar.sh. The code-side wiring (license backend, trial state, in-app activation UI, Tauri updater, GitHub Actions release workflow, entitlements file) is already in place — these are the human-driven prerequisites.

## 1. Apple Developer Program (longest lead time — start first)

1. Enroll at <https://developer.apple.com/programs/> (~$99/yr, approval can take 24–48 hours).
2. Once approved, in Apple Developer → Certificates, Identifiers & Profiles, create a **Developer ID Application** certificate. Download and double-click to install in Keychain Access.
3. Generate an **app-specific password** at <https://account.apple.com/account/manage> → Sign-In and Security → App-Specific Passwords. Label it `tucknotes-notarytool`.
4. Note the following — they go into GitHub Actions secrets, not the repo:
   - **`APPLE_ID`** — your Apple Developer email
   - **`APPLE_PASSWORD`** — the app-specific password from step 3
   - **`APPLE_TEAM_ID`** — the 10-character Team ID from <https://developer.apple.com/account#MembershipDetailsCard>
   - **`APPLE_SIGNING_IDENTITY`** — the Common Name of the cert, e.g. `Developer ID Application: Andre Gagnon (TEAMID12)`. Run `security find-identity -v -p codesigning` to print it.
5. Export the cert as `.p12` for CI:
   - In Keychain Access → My Certificates, right-click the Developer ID Application cert → Export. Save as `.p12` with a strong password.
   - Base64 it: `base64 -i tucknotes-cert.p12 | pbcopy` → this string goes into the GitHub secret `APPLE_CERTIFICATE`.
   - The export password goes into `APPLE_CERTIFICATE_PASSWORD`.

## 2. Polar.sh

1. Create an organization at <https://polar.sh>.
2. Create a one-time product: "TuckNotes — Lifetime License" (your price).
3. On that product, **enable License Keys**. Set `activation_limit = 1` (one device per key — users with a new machine deactivate from Settings to free the slot).
4. From your Polar org settings, copy the **Organization ID** (it's a UUID). This is public-safe — it goes into the build env var `POLAR_ORGANIZATION_ID`.
5. Note your product's hosted checkout URL (something like `https://buy.polar.sh/<slug>`). Update `BUY_URL` in `src/features/licensing/types.ts` to match.

## 3. Tauri updater signing keypair

Run **once** on your local machine:

```bash
mkdir -p ~/.tauri
npm run tauri signer generate -- -w ~/.tauri/tucknotes-updater.key
```

You'll be prompted for a password. Save it in a password manager.

The command prints a public key — copy that into `src-tauri/tauri.conf.json` at `plugins.updater.pubkey`, replacing `REPLACE_WITH_TAURI_UPDATER_PUBKEY`.

The private key file (`~/.tauri/tucknotes-updater.key`) and its password go into GitHub Actions secrets:

- **`TAURI_SIGNING_PRIVATE_KEY`** — contents of the `.key` file
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`** — the password you chose

## 4. GitHub repository setup

1. Create the GitHub repo and push this codebase.
2. In `src-tauri/tauri.conf.json`, update `plugins.updater.endpoints` — replace `REPLACE_OWNER/REPLACE_REPO` with your actual `owner/repo`.
3. Configure Actions secrets at `https://github.com/<owner>/<repo>/settings/secrets/actions`. Add:
   - `APPLE_CERTIFICATE`
   - `APPLE_CERTIFICATE_PASSWORD`
   - `APPLE_ID`
   - `APPLE_PASSWORD`
   - `APPLE_TEAM_ID`
   - `APPLE_SIGNING_IDENTITY`
   - `TAURI_SIGNING_PRIVATE_KEY`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   - `POLAR_ORGANIZATION_ID`

## 5. Cutting a release

The pipeline is triggered by **creating a GitHub Release as a pre-release**. It
stays a pre-release (and so stays out of GitHub's `/releases/latest/` redirect)
for the entire build, and is only promoted to "Latest" after every asset uploads
successfully. This guarantees the stable download link and the Tauri updater —
which both resolve through `…/releases/latest/download/…` — never point at an
assetless release while a build is in progress or has failed.

1. Create the release as a **pre-release** (this also creates the `vX.Y.Z` git tag;
   a _draft_ would not, and would break the bump step). Either:
   - Web UI → Draft a new release → tag `vX.Y.Z` → **check "Set as a pre-release"** → Publish; or
   - `gh release create vX.Y.Z --prerelease --title "vX.Y.Z" --notes "…"`

   You do **not** need to bump version files by hand — `bump.yml` does that.

2. `bump.yml` fires on `release: published`. It demotes the release to a
   pre-release (a no-op given step 1, but a safety net), bumps `version` in
   `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and
   `src-tauri/Cargo.lock`, force-moves the tag onto the bump commit, fast-forwards
   `master`, and dispatches `release.yml`.
3. `release.yml` runs on the moved tag, builds for `aarch64-apple-darwin` (Apple
   Silicon only — Intel is unsupported because the bundled inference stack requires
   Metal on M-series GPUs), signs and notarizes, and uploads the `.dmg`,
   `.app.tar.gz`, signed `latest.json`, and the versionless `TuckNotes-arm64.dmg`.
   As its final step it promotes the release to non-pre-release + **Latest**. The
   Tauri updater in shipped builds then picks up the new `latest.json` automatically.
4. If the build fails, the release stays a pre-release, so `/releases/latest/`
   keeps serving the previous release's DMG and `latest.json`. Fix the issue and
   re-run `release.yml` on the tag (`gh workflow run release.yml --ref vX.Y.Z`); it
   promotes to latest on success.

## End-to-end smoke test

Once the first release is out:

1. Install the published `.dmg` on a clean account (or remove the app and `~/Library/Application Support/com.andre.tucknotes/` first).
2. App launches → Settings shows "Trial · 14 days remaining".
3. To simulate trial expiry without waiting: edit `~/Library/Application Support/com.andre.tucknotes/license.json` and set `first_launch_at` to a value 15+ days in the past (`echo $(( $(date +%s) - 15*86400 ))`). Restart the app. Settings shows "Trial expired"; the recording button routes to Settings instead of starting capture; the summarize action errors with `LicenseRequired`.
4. Buy a license through Polar (use a 100% discount code if you don't want to charge yourself), paste the key into Settings → Activate. Status flips to "Licensed".
5. From the Polar dashboard, deactivate the activation. Wait 60s for the polling hook, then click "Check for updates" → after the next revalidate cycle the status flips to `LicenseInvalid`.

## Files referenced

- `src-tauri/tauri.conf.json` — bundle config, updater endpoint + pubkey
- `src-tauri/Entitlements.plist` — hardened-runtime entitlements
- `src-tauri/src/services/licensing.rs` — Polar API client, `POLAR_ORGANIZATION_ID` constant
- `src-tauri/src/models/licensing.rs` — `TRIAL_DAYS = 14`, `OFFLINE_GRACE_DAYS = 7`
- `src/features/licensing/types.ts` — `BUY_URL`
- `.github/workflows/release.yml` — CI release pipeline
