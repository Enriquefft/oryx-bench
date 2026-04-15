{
  description = "oryx-bench — a workbench for ZSA keyboard layouts";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Crane builds the crate in two derivations: a deps-only layer
    # (cached, changes only when Cargo.lock changes) plus the workspace
    # crate itself. Path-flake iterations rebuild only the second
    # layer — ~10min cold starts drop to seconds. Also filters
    # `target/` out of the store copy even on dirty trees.
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
    }:
    let
      # Read the package metadata from Cargo.toml so the flake's
      # version always matches what cargo builds. Single source of
      # truth — no hand-maintained version constants in this file.
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      # ZSA's `zapp` CLI — the flash backend oryx-bench delegates to.
      # Pinned to a specific commit so the package hash is
      # deterministic and nix evaluation stays offline-capable.
      # zapp is not in nixpkgs (yet); we package it here as part of
      # oryx-bench's runtime closure so a single `nix profile install`
      # (or NixOS module enable) ships both binaries together.
      #
      # Bumping: change `zappRev` to the new commit SHA, then set
      # `zappHash` to "" and rebuild — nix will print the correct
      # sha256 to paste back here. Same for `cargoHash`.
      zappRev = "aaffabf80e9e5c003b53d92163787b6c47906788"; # v1.0.0, 2026-04-07
      zappHash = "sha256-OBYElUfLTm/TI4rB6oosSC7DT/39yUuav093IjTJzlU=";
      mkZapp =
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          zappSrc = pkgs.fetchFromGitHub {
            owner = "zsa";
            repo = "zapp";
            rev = zappRev;
            hash = zappHash;
          };
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = "zapp";
          version = "1.0.0-git-${builtins.substring 0 7 zappRev}";
          src = zappSrc;
          cargoLock.lockFile = "${zappSrc}/Cargo.lock";
          # nusb (pure-Rust USB, no libusb subprocess) + rustls handle
          # the heavy lifting — no pkg-config, no native libs needed.
          doCheck = false;
          meta = with pkgs.lib; {
            description = "ZSA's official flasher for their keyboards";
            homepage = "https://github.com/zsa/zapp";
            # Upstream LICENSE is MIT with a Commons Clause rider
            # that only restricts commercial resale of the software
            # as a service — irrelevant for end-user flashing. The
            # underlying grant is MIT; GitHub reports NOASSERTION
            # because of the rider, not the core license.
            license = licenses.mit;
            mainProgram = "zapp";
          };
        };
    in
    flake-utils.lib.eachDefaultSystem (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          # Use the exact same toolchain binaries that CI gets from
          # dtolnay/rust-toolchain@stable. This eliminates formatting
          # and lint divergence between local dev and CI.
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
          };

          # Crane lib pinned to the same toolchain as the devShell —
          # one Rust version across interactive dev, nix-built
          # artifacts, and CI.
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # GUI + hidapi runtime lib set — listed once, referenced by
          # buildInputs, postFixup rpath, and the devShell hook.
          guiLibs = with pkgs; [
            libGL
            libxkbcommon
            wayland
            fontconfig
            libx11
            libxcursor
            libxi
            libxrandr
            libxcb
            # `oryx-bench watch` uses hidapi (C implementation, linux-
            # static-hidraw feature) to open the ZSA keyboard's raw-HID
            # endpoint. hidapi itself statically links, but its
            # enumerator calls into libudev to walk /sys/class/hidraw/.
            udev
          ];

          # Shared crane args between the deps-only layer and the
          # final crate build. Keep lean — anything that varies per
          # derivation (pname, version, postFixup, meta) goes on the
          # individual build, not here, otherwise deps cache misses.
          #
          # Source filter: gitignore-aware so `target/`, `result/`,
          # `.oryx-bench/` etc. are dropped from the store copy, while
          # non-rust assets referenced by `include_str!` (skills/,
          # packaging/docker/, examples/) ride along. craneLib's
          # default `cleanCargoSource` is rust-only and breaks those
          # embeds.
          commonArgs = {
            src = pkgs.nix-gitignore.gitignoreSource [ ] ./.;
            strictDeps = true;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = guiLibs;
            doCheck = false;
          };

          # Compile just the dependency graph. Cached in the nix
          # store keyed on Cargo.lock + toolchain — reused across
          # every workspace code change until a dep is bumped.
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          oryx-bench = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = cargoToml.package.name;
              version = cargoToml.package.version;
              # glow/winit dlopen the GL + wayland libs at runtime;
              # hidapi dlopens libudev.so.1 for /sys/class/hidraw/
              # enumeration. Bake the paths into rpath so the binary
              # works from any PATH without LD_LIBRARY_PATH.
              postFixup = ''
                patchelf --set-rpath "${pkgs.lib.makeLibraryPath guiLibs}" $out/bin/oryx-bench
              '';
              meta = with pkgs.lib; {
                description = cargoToml.package.description;
                homepage = cargoToml.package.homepage;
                license = licenses.mit;
                mainProgram = "oryx-bench";
              };
            }
          );

          zapp = mkZapp system;
        in
        {
          devShells.default = pkgs.mkShell {
            name = "oryx-bench-dev";
            packages = [
              # Single unified Rust toolchain — rustc, cargo, rustfmt,
              # clippy, rust-analyzer all from the same official release.
              rustToolchain

              # Build-time deps for reqwest/rustls (used by pull/)
              pkgs.pkg-config

              # `oryx-bench watch` links eframe/egui for the indicator
              # window; the libs below are linked at build time and
              # dlopened at runtime by glow/winit. libudev is linked
              # for hidapi's /sys/class/hidraw/ enumeration.
            ]
            ++ guiLibs
            ++ [

              # Generally useful
              pkgs.git
              pkgs.jq

              # Python deps for `qmk` — used by the codegen round-trip
              # test when `qmk` is on PATH.
              (pkgs.python3.withPackages (
                ps: with ps; [
                  appdirs
                  hjson
                  jsonschema
                  milc
                  pygments
                  dotty-dict
                  pillow
                ]
              ))

              # Zig for Tier 2 overlay code; available in the shell so
              # manual experiments match the docker image.
              pkgs.zig

              # Flashing tools.
              # `zapp` (https://github.com/zsa/zapp) is ZSA's official
              # flasher and the only one oryx-bench talks to. Built
              # from source via this flake (not in nixpkgs yet) — the
              # same derivation the NixOS module ships.
              zapp

              # Git hooks
              pkgs.lefthook
            ];

            # Keep target/ out of the Nix store to allow incremental builds.
            shellHook = ''
              export CARGO_TARGET_DIR="$PWD/target"
              : "''${RUST_LOG:=warn}"
              export RUST_LOG
              # Single source of truth for the runtime-linked lib set
              # used by `oryx-bench watch`'s GUI (winit + glow dlopen
              # these). Exported twice:
              #   - LD_LIBRARY_PATH: so `cargo run` inside the shell
              #     resolves them without needing rpath yet.
              #   - ORYX_RUNTIME_RPATH: consumed by build.rs to bake
              #     the same paths into the binary's rpath, so
              #     `./target/release/oryx-bench` invoked from outside
              #     the shell (e.g. a user's normal terminal) still
              #     finds them. Matches what `nix run` does via postFixup.
              __ORYX_GUI_LIBS="${pkgs.lib.makeLibraryPath guiLibs}"
              export LD_LIBRARY_PATH="$__ORYX_GUI_LIBS:$LD_LIBRARY_PATH"
              export ORYX_RUNTIME_RPATH="$__ORYX_GUI_LIBS"
            '';
          };

          # `nix build` builds the real oryx-bench binary.
          # `nix run .#oryx-bench -- --help` runs it.
          packages = {
            default = oryx-bench;
            oryx-bench = oryx-bench;
            zapp = zapp;
          };

          apps.default = {
            type = "app";
            program = "${oryx-bench}/bin/oryx-bench";
          };
        }
      ) // {
        # Support for `nix flake show` / `nix flake check`
        nixosModules = {
          default = import ./packaging/nix/module.nix;
          oryx-bench = import ./packaging/nix/module.nix;
        };
      };
}
