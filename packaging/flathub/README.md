# Submitting Telinhas to Flathub

Flathub is where the COSMIC Store gets its apps. Official guide:
<https://docs.flathub.org/docs/for-app-authors/submission>.

## First submission

1. Make sure the release tag referenced in `io.github.ninjairocode.Telinhas.yml`
   (`tag` and `commit`) is pushed to GitHub.
2. Fork <https://github.com/flathub/flathub> **including all branches**
   (untick "Copy the master branch only").
3. Clone the fork on the `new-pr` branch and create a branch for the app:
   ```
   git clone --branch=new-pr git@github.com:<your-user>/flathub.git
   cd flathub
   git checkout -b io.github.ninjairocode.Telinhas new-pr
   ```
4. Copy the files to the root of that repository:
   ```
   cp <telinhas>/packaging/flathub/io.github.ninjairocode.Telinhas.yml .
   cp <telinhas>/packaging/flatpak/cargo-sources.json .
   git add . && git commit -m "Add io.github.ninjairocode.Telinhas" && git push -u origin HEAD
   ```
5. Open a pull request **against the `new-pr` branch** of `flathub/flathub`,
   titled `Add io.github.ninjairocode.Telinhas`, and use the text below.
6. Reviewers comment on the PR; a test build starts when someone writes
   `bot, build`. Answer their questions and push fixes to the same branch.
7. After approval you get a repository `flathub/io.github.ninjairocode.Telinhas`.
   Log in to flathub.org with GitHub to verify the `io.github.ninjairocode` ID.

## Updates

For each new release: regenerate `cargo-sources.json` if `Cargo.lock`
changed, open a PR on `flathub/io.github.ninjairocode.Telinhas` updating `tag`,
`commit` and `cargo-sources.json`. Once merged, users get the update through the
COSMIC Store / `flatpak update`.

## Pull request text

```
Telinhas manages the small screens found in PCs (AIO cooler displays, case
panels). It shows images, animated GIFs and system metrics on them. Native
COSMIC app (libcosmic), GPL-3.0-or-later, written by me.

Upstream: https://github.com/ninjairocode/telinhas-project

Permissions:
- --device=all: the screens are USB HID devices (/dev/hidraw*); there is no
  narrower device permission for hidraw. Users install a udev rule on the
  host (documented in the app and README), as with other hardware apps.
- --filesystem=xdg-config/autostart:create: "Start with the system". The app
  first asks the Background portal; COSMIC does not implement it, so as a
  fallback the app writes only its own .desktop file there.
- --talk-name=com.system76.CosmicSettingsDaemon and
  --filesystem=xdg-config/cosmic:ro: COSMIC theme, as other libcosmic apps.

- [x] I am the upstream developer.
- [x] The app builds and runs locally with flatpak-builder.
```
