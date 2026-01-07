# Nix Setup

Fedimint uses [Nix](https://nixos.org/) for building, CI, and managing the development environment.

::: tip
Only **Nix** (the language & package manager) is needed. You **do not** need to install NixOS (the Linux distribution). Nix can be installed on most Linux distributions and macOS.
:::

While it is technically possible to develop without Nix, it is **highly recommended** as it ensures a consistent and reproducible environment for all developers, matching the CI environment.

## Install Nix

You have two main options to install Nix:

### Option 1. The Determinate Nix Installer (Recommended)

This is a modern, opinionated installer that sets up everything ensuring a smooth experience.

[Determinate Systems Nix Installer](https://github.com/DeterminateSystems/nix-installer)

### Option 2. The Official Installer

[Download Nix / NixOS](https://nixos.org/download.html)

After installation, verify it:

```bash
nix --version
# Example output: nix (Nix) 2.18.1
```

### Enable Flakes

If you used the **Determinate Nix Installer**, flakes are likely already enabled.

If you used the **Official Installer**, you may need to enable flakes manually. Edit `~/.config/nix/nix.conf` or `/etc/nix/nix.conf` and add:

```ini
experimental-features = nix-command flakes
```

If you encounter issues, don't forget to restart the nix-daemon.

## Development Shell

Once Nix is set up, you can enter the project's development environment.

```bash
nix develop
```

This command will download all necessary dependencies and tools (Rust, Node.js, etc.) and drop you into a shell where they are available in your path.

::: info
The first time you run `nix develop`, it may take a significant amount of time to download and cache all dependencies. Subsequent runs will be instant.
:::

### Using a Custom Shell

By default `nix develop` uses a bash shell. If you prefer to use your own shell (like zsh or fish), you can run:

```bash
nix develop -c zsh
```

**Note:** All development commands (building, testing, generating code) should be run inside this `nix develop` shell to ensure you are using the correct versions of all tools.
