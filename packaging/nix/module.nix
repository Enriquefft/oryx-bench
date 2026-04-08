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

    # Optionally generate flash scripts
    environment.systemPackages =
      if cfg.enableFlashScripts then
        [ cfg.package ] ++ [
          (pkgs.runCommandLocal "oryx-bench-flash-scripts" { } ''
            mkdir -p $out/bin
            ${concatStringsSep "\n" (
              mapAttrsToList (name: layout:
                if layout.enable then
                  "cp ${flashScript name layout}/bin/flash-${name} $out/bin/"
                else ""
              ) cfg.keyboards
            )}
          '')
        ]
      else
        [ cfg.package ];
  };

  meta.maintainers = [ ];
}
