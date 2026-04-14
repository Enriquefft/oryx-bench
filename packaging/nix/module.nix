{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.oryx-bench;

  # Build a single keyboard layout using oryx-bench
  buildKeyboardLayout = name: layoutCfg: pkgs.stdenvNoCC.mkDerivation {
    pname = "oryx-bench-${name}";
    version = layoutCfg.version or "0.1.0";

    src = layoutCfg.source;

    nativeBuildInputs = [
      cfg.package
      pkgs.docker
    ];

    buildPhase = ''
      cd $src
      oryx-bench build --no-pull 2>&1
    '';

    installPhase = ''
      mkdir -p $out/firmware
      cp .oryx-bench/build/firmware.bin $out/firmware/
      echo "${name}" > $out/KEYBOARD
      echo "${layoutCfg.version or "0.1.0"}" > $out/VERSION

      # Optionally generate a markdown summary
      mkdir -p $out/share/doc
      oryx-bench show > $out/share/doc/layout.txt 2>&1 || true
      oryx-bench lint > $out/share/doc/lint-report.txt 2>&1 || true
    '';

    doCheck = true;
    checkPhase = ''
      cd $src
      oryx-bench lint
    '';
  };

  # Script to flash a keyboard layout
  flashScript = name: layoutCfg: pkgs.writeShellScriptBin "flash-${name}" ''
    set -euo pipefail

    echo "Flashing ${name} keyboard..."

    layout_dir="$(cd "$(dirname "''${BASH_SOURCE[0]}")/../.." && pwd)"
    cd "$layout_dir"

    if [[ -n "''${DRY_RUN:-}" ]]; then
      echo "DRY RUN: Would flash with:"
      ${cfg.package}/bin/oryx-bench flash --dry-run
    else
      ${cfg.package}/bin/oryx-bench flash --yes
    fi
  '';

in
{
  options.programs.oryx-bench = {
    enable = mkEnableOption "oryx-bench keyboard workbench";

    package = mkOption {
      type = types.package;
      default = pkgs.oryx-bench or (import ../.. { inherit pkgs; });
      defaultText = "pkgs.oryx-bench (from oryx-bench repo)";
      description = "The oryx-bench package to use";
    };

    zappPackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      defaultText = "null — set from the flake so `zapp` lands on PATH";
      description = ''
        The `zapp` package to install alongside oryx-bench. `oryx-bench
        flash` shells out to `zapp`, so the CLI is a hard runtime
        dependency. The oryx-bench flake exposes a derivation at
        `packages.''${system}.zapp` — pass it here to ship a matched
        pair. Set to `null` to skip (e.g. if you install zapp some
        other way).
      '';
    };

    keyboards = mkOption {
      type = types.attrsOf (types.submodule {
        options = {
          enable = mkEnableOption "this keyboard layout" // { default = true; };

          source = mkOption {
            type = types.path;
            description = "Path to the keyboard layout directory (contains kb.toml, layout.toml, overlay/)";
            example = "/etc/nixos/keyboards/voyager";
          };

          version = mkOption {
            type = types.str;
            default = "0.1.0";
            description = "Version string for this keyboard layout";
          };

          autoFlash = mkOption {
            type = types.bool;
            default = false;
            description = ''
              Automatically flash the keyboard after building.
              WARNING: Only enable if you have a single keyboard and know what you're doing.
            '';
          };
        };
      });
      default = { };
      description = "Keyboard layouts to build";
      example = {
        voyager = {
          source = "/etc/nixos/keyboards/voyager";
          autoFlash = false;
        };
      };
    };

    enableFlashScripts = mkOption {
      type = types.bool;
      default = true;
      description = "Generate convenient flash-<name> scripts in /nix/var/nix/profiles/system/bin/";
    };
  };

  config = mkIf cfg.enable {

    # `oryx-bench watch` speaks the Oryx WebHID protocol directly to
    # the keyboard over `/dev/hidraw*`. That interface is root-only by
    # default on Linux; the user needs the ZSA udev rules to reach it.
    #
    # Nixpkgs already ships `hardware.keyboard.zsa` (see
    # `nixos/modules/hardware/keyboard/zsa.nix`) which vendors the
    # maintained `zsa-udev-rules` package — the same `zsa/wally`-
    # derived ruleset we seed at `packaging/linux/50-zsa.rules` for
    # non-NixOS distros. Enabling the upstream option is the single-
    # source-of-truth path; re-vendoring the rule file into the Nix
    # store would duplicate state.
    #
    # `mkDefault` (not `mkForce`) so users who manage ZSA udev rules
    # themselves can opt out with
    # `hardware.keyboard.zsa.enable = false;`.
    hardware.keyboard.zsa.enable = lib.mkDefault true;

    # Build all enabled keyboard layouts and expose them
    system.extraSystemBuilderCmds =
      let
        enabledLayouts = filterAttrs (name: layout: layout.enable) cfg.keyboards;
        builds = mapAttrs buildKeyboardLayout enabledLayouts;
      in
      ''
        mkdir -p $out/share/oryx-bench/keyboards
        ${concatStringsSep "\n" (
          mapAttrsToList (name: drv:
            "cp -r ${drv} $out/share/oryx-bench/keyboards/${name}"
          ) builds
        )}
      '';

    # `zapp` is a hard runtime dep of `oryx-bench flash`; ship it in
    # the same closure as the CLI so `nixos-rebuild switch` is
    # sufficient to get both binaries on PATH. `zappPackage = null`
    # opts out for users installing zapp through another channel.
    environment.systemPackages =
      let
        core = [ cfg.package ] ++ optional (cfg.zappPackage != null) cfg.zappPackage;
        scripts = optional cfg.enableFlashScripts (
          pkgs.runCommandLocal "oryx-bench-flash-scripts" { } ''
            mkdir -p $out/bin
            ${concatStringsSep "\n" (
              mapAttrsToList (name: layout:
                if layout.enable then
                  "cp ${flashScript name layout}/bin/flash-${name} $out/bin/"
                else ""
              ) cfg.keyboards
            )}
          ''
        );
      in
      core ++ scripts;
  };

  meta.maintainers = [ ];
}
