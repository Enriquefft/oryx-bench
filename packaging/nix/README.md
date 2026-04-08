# NixOS Integration for oryx-bench

This directory contains NixOS module definitions and Nix packaging for oryx-bench, enabling declarative keyboard layout management through NixOS configuration.

## Quick Start

### 1. Add oryx-bench to your flake.nix

```nix
{
  description = "My NixOS configuration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager.url = "github:nix-community/home-manager";
    oryx-bench.url = "github:enriquefft/oryx-bench";
  };

  outputs = { self, nixpkgs, home-manager, oryx-bench }:
    {
      nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ./configuration.nix
          oryx-bench.nixosModules.default
        ];
        specialArgs = { inherit oryx-bench; };
      };
    };
}
```

### 2. Create your keyboard layout directory

```bash
mkdir -p /etc/nixos/keyboards/voyager
oryx-bench init --blank --geometry voyager --directory /etc/nixos/keyboards/voyager
```

### 3. Configure in your NixOS config

```nix
# /etc/nixos/configuration.nix
{
  programs.oryx-bench = {
    enable = true;

    keyboards.voyager = {
      enable = true;
      source = /etc/nixos/keyboards/voyager;
    };
  };
}
```

### 4. Build and deploy

```bash
sudo nixos-rebuild switch
```

The firmware will be built, linted, and available at:
```
/run/current-system/share/oryx-bench/keyboards/voyager/firmware/firmware.bin
```

## Module Options

### `programs.oryx-bench.enable`

Enable the oryx-bench module.

**Type**: `bool`
**Default**: `false`

### `programs.oryx-bench.package`

The oryx-bench package to use.

**Type**: `package`
**Default**: `pkgs.oryx-bench` (from the flake input)

You can override this to use a specific version:

```nix
programs.oryx-bench.package = oryx-bench.packages.${pkgs.system}.default;
```

### `programs.oryx-bench.keyboards.<name>.source`

Path to the keyboard layout directory (containing `kb.toml`, `layout.toml`, `overlay/`).

**Type**: `path`
**Example**: `/etc/nixos/keyboards/voyager`

This directory should be created with `oryx-bench init --blank --geometry <geometry>`.

### `programs.oryx-bench.keyboards.<name>.version`

Version string for the keyboard layout (used in build derivation).

**Type**: `string`
**Default**: `"0.1.0"`

### `programs.oryx-bench.keyboards.<name>.enable`

Enable building this keyboard layout.

**Type**: `bool`
**Default**: `true`

### `programs.oryx-bench.enableFlashScripts`

Generate convenient `flash-<name>` scripts.

**Type**: `bool`
**Default**: `true`

When enabled, scripts are available in `/run/current-system/bin/`:
```bash
flash-voyager        # Flashes the Voyager layout
DRY_RUN=1 flash-voyager  # Dry run
```

## Full Example Configuration

```nix
# /etc/nixos/configuration.nix
{
  imports = [ ... ];

  programs.oryx-bench = {
    enable = true;

    # Multiple keyboards can be configured
    keyboards = {
      voyager = {
        enable = true;
        source = /etc/nixos/keyboards/voyager;
        version = "0.1.0";
      };

      # Future: moonlander configuration
      # moonlander = {
      #   enable = false;
      #   source = /etc/nixos/keyboards/moonlander;
      # };
    };

    enableFlashScripts = true;
  };

  # Other system configuration...
}
```

## Version Control

The recommended structure for your `/etc/nixos` git repository:

```
/etc/nixos/
├── flake.nix                # Points to oryx-bench input
├── configuration.nix         # Main system config
├── keyboards/
│   └── voyager/
│       ├── kb.toml          # Project config
│       ├── layout.toml       # Visual layout (single source of truth)
│       ├── overlay/
│       │   ├── features.toml # Tier 1: declarative QMK features
│       │   └── *.zig         # Tier 2: procedural code
│       └── .gitignore        # Ignore build artifacts
└── .git/
```

All keyboard layouts are version-controlled, giving you full git history of layout changes.

## Building Firmware Manually

Even with the NixOS module, you can still use oryx-bench directly:

```bash
cd /etc/nixos/keyboards/voyager
oryx-bench show       # View the layout
oryx-bench lint       # Check for issues
oryx-bench build      # Build firmware.bin
oryx-bench flash      # Flash to keyboard (with Keymapp or wally-cli)
```

## Building Without NixOS

The NixOS module is optional. You can use oryx-bench on any Linux/macOS system:

```bash
# Install via Nix (development flake)
nix run github:enriquefft/oryx-bench -- --help

# Or build directly from the repository
cargo build --release
./target/release/oryx-bench init --blank --geometry voyager
./target/release/oryx-bench build
```

## Reproducible Builds

All components of the build are pinned for reproducibility:

1. **oryx-bench version**: Fixed by the flake input
2. **qmk_firmware version**: Fixed in `packaging/docker/pin.txt`
3. **Zig version**: Fixed in the Docker image (0.13.0)
4. **GCC version**: Fixed by nixpkgs version
5. **Layout source**: Version-controlled in `/etc/nixos/keyboards/`

Two machines with identical NixOS configs and git commits will build byte-identical firmware.

## Troubleshooting

### Build fails with "Docker not available"

Ensure Docker is installed and running:
```bash
sudo systemctl start docker
sudo usermod -aG docker $USER
```

### "oryx-bench command not found" after nixos-rebuild

The command is in `/run/current-system/bin/oryx-bench`. Add to your shell:
```nix
environment.systemPackages = [ pkgs.oryx-bench ];
```

Or use the full path:
```bash
/run/current-system/bin/oryx-bench --help
```

### Firmware build is slow

Builds are cached. Subsequent builds with the same `layout.toml` and `overlay/` should be instant. To force a rebuild:
```bash
rm -rf /etc/nixos/keyboards/voyager/.oryx-bench/build
nixos-rebuild switch
```

### "Permission denied" when running flash scripts

Make sure you're in the `dialout` group:
```bash
sudo usermod -aG dialout $USER
# Log out and back in
```

## Advanced: Custom Build Options

To add custom build options (e.g., different flash backend), modify the keyboard derivation:

```nix
programs.oryx-bench.keyboards.voyager.buildInputs = [
  pkgs.wally-cli  # If available in nixpkgs
];
```

## Architecture Alignment

The NixOS module respects the oryx-bench architecture:

- **Tier 0** (visual layout): `layout.toml` in `/etc/nixos/keyboards/`
- **Tier 1** (declarative features): `overlay/features.toml`
- **Tier 2** (procedural code): `overlay/*.zig`
- **Build cache**: `.oryx-bench/build/` (gitignored)
- **Firmware**: Built in Nix sandbox, pinned versions

See [ARCHITECTURE.md](../../ARCHITECTURE.md) for the full design.

## Contributing

Improvements to the NixOS module are welcome. Please ensure:
- Changes align with [CLAUDE.md](../../CLAUDE.md) principles
- The module works with both `nixos-rebuild` and direct `nix build`
- Documentation is updated
