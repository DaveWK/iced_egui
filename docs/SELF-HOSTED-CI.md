# Self-hosted Fedora / RHEL CI

CI runs directly on an x86_64 Fedora or RHEL-family machine. There are no
Ubuntu jobs or containers. The required runner labels are:

```text
self-hosted, linux, x64, redhat, iced-egui
```

Use a dedicated machine or VM and an unprivileged runner account without
production credentials. The workflow runs only for pushes to this
repository's main branch or manual dispatches of main. Pull requests do
not execute automatically on the host; contributors can run the checks
locally, with CI running after reviewed changes land on main. The workflow
token has read-only repository contents access.

## Prepare the host

An administrator installs the build dependencies once:

```sh
sudo dnf install -y git curl tar gzip unzip zstd gcc gcc-c++ make cmake \
  pkgconf-pkg-config openssl-devel fontconfig-devel wayland-devel \
  libxkbcommon-devel mesa-vulkan-drivers vulkan-loader
```

On RHEL, enable the appropriate subscribed repositories if a development
package is unavailable. The chosen host must be able to download the
GitHub runner, Rust toolchain and Cargo dependencies.

Install and register the Linux x64 GitHub Actions runner using the commands
shown under repository **Settings → Actions → Runners → New self-hosted
runner**. Run its configuration as the dedicated account and add the custom
labels `redhat,iced-egui`. Keep the default `self-hosted,linux,x64` labels.
Install its service with that same account as the service user.

Registration tokens are short-lived; obtain one during installation rather
than saving it in this repository. No permanent GitHub personal access token
is needed by the build jobs.

## Checks

The workflow verifies the OS, build tools and pkg-config libraries before
checkout. It installs Rust stable, rustfmt, clippy and the wasm32 target
through the toolchain action. It runs formatting, clippy, feature-disabled
compilation, unit tests, doctests, Vulkan integration tests, rustdoc and
wasm32 compilation.

Mesa lavapipe is selected through `VK_DRIVER_FILES`, discovered from the
installed x86_64 Mesa RPM. The Vulkan test uses real Vulkan rendering and
pixel readback, but does not need a physical GPU or a desktop session.
The runner account needs write access to its work and temporary directories;
build steps do not use sudo.

## Verify registration

```sh
gh api repos/DaveWK/iced_egui/actions/runners \
  --jq '.runners[] | {name,status,labels: [.labels[].name]}'
gh workflow run ci.yml --repo DaveWK/iced_egui --ref main
gh run list --repo DaveWK/iced_egui --workflow ci.yml
```

An online runner with all five labels must be present. Until registration
is complete, jobs will queue rather than falling back to a hosted runner.
