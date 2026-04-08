{
  description = "oryx-bench — a workbench for ZSA keyboard layouts";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      # Read the package metadata from Cargo.toml so the flake's
      # version always matches what cargo builds. Single source of
      # truth — no hand-maintained version constants in this file.
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      # Helper to build oryx-bench for a given system
      mkOryx = system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ ];
          doCheck = false;
          meta = with pkgs.lib; {
            description = cargoToml.package.description;
            homepage = cargoToml.package.homepage;
            license = licenses.mit;
            mainProgram = "oryx-bench";
          };
        };
    in
    {
      # Support for `nix flake show` / `nix flake check`
      nixosModules = {
        default = import ./packaging/nix/module.nix;
        oryx-bench = import ./packaging/nix/module.nix;
      };

      # Per-system outputs (dev shell, packages, apps)
      outputs = flake-utils.lib.eachDefaultSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };
          oryx-bench = mkOryx system;
        in
        {
          devShells.default = pkgs.mkShell {
            name = "oryx-bench-dev";
            packages = with pkgs; [
              # Rust toolchain for the CLI itself
              rustc
              cargo
              rustfmt
              clippy
              rust-analyzer

              # Build-time deps for reqwest/rustls (used by pull/)
              pkg-config

              # Generally useful
              git
              jq

              # Python deps for `qmk` — used by the codegen round-trip
              # test when `qmk` is on PATH.
              (python3.withPackages (ps: with ps; [
                appdirs
                hjson
                jsonschema
                milc
                pygments
                dotty-dict
                pillow
              ]))

              # Zig for Tier 2 overlay code; available in the shell so
              # manual experiments match the docker image.
              zig

              # Flashing tools.
              # wally-cli is not currently in nixpkgs under that exact name;
              # users who need it can install via the ZSA installer.
            ];

            # Keep target/ out of the Nix store to allow incremental builds.
            shellHook = ''
              export CARGO_TARGET_DIR="$PWD/target"
              : "''${RUST_LOG:=warn}"
              export RUST_LOG
            '';
          };

          # `nix build` builds the real oryx-bench binary.
          # `nix run .#oryx-bench -- --help` runs it.
          packages = {
            default = oryx-bench;
            oryx-bench = oryx-bench;
          };

          apps.default = {
            type = "app";
            program = "${oryx-bench}/bin/oryx-bench";
          };
        }
      );
    };
}
