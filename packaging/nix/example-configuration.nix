# Example NixOS configuration using oryx-bench
# Copy this into your /etc/nixos/configuration.nix and adapt as needed

{ config, lib, pkgs, ... }:

{
  imports = [
    # ... other imports
    # ./hardware-configuration.nix
  ];

  # ============================================================================
  # oryx-bench: Keyboard layout management
  # ============================================================================
  programs.oryx-bench = {
    enable = true;

    # The oryx-bench package (defaults to latest from the input flake)
    # package = pkgs.oryx-bench;

    # Define all keyboards to be managed through NixOS
    keyboards = {
      # Voyager keyboard (main layout)
      voyager = {
        enable = true;
        source = /etc/nixos/keyboards/voyager;
        version = "0.1.0";

        # Uncomment to auto-flash after every rebuild
        # WARNING: Only safe if you have exactly one keyboard plugged in
        # autoFlash = false;
      };

      # Example: Moonlander (disabled for now)
      # moonlander = {
      #   enable = false;
      #   source = /etc/nixos/keyboards/moonlander;
      #   version = "0.1.0";
      # };
    };

    # Generate convenient flash-<name> scripts
    enableFlashScripts = true;
  };

  # ============================================================================
  # Example: Access flashing scripts in your shell
  # ============================================================================
  # After `nixos-rebuild switch`, you can:
  #   flash-voyager                # Flashes the keyboard
  #   DRY_RUN=1 flash-voyager     # Dry run (shows what would happen)
  #
  # Scripts are available at:
  #   /run/current-system/bin/flash-voyager
  #   /run/current-system/bin/flash-moonlander
  #   ...

  # ============================================================================
  # Example: HID rules for keyboard access
  # ============================================================================
  # Allow unprivileged users to flash keyboards (optional)
  services.udev.extraRules = ''
    # ZSA Voyager and other ZSA keyboards
    SUBSYSTEMS=="usb", ATTRS{idVendor}=="3297", MODE:="0666"
    # Generic DFU bootloader
    SUBSYSTEMS=="usb", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="df11", MODE:="0666"
  '';

  # Or use the standard group-based approach (recommended)
  users.groups.dialout.members = [ "hybridz" ];  # Add your username

  # ============================================================================
  # Example: Home-manager integration (if you use it)
  # ============================================================================
  # You can access keyboard artifacts from home-manager:
  #
  # home-manager.users.hybridz = { config, ... }: {
  #   home.activation.oryx-bench = config.lib.dag.entryAfter ["writeBoundary"] ''
  #     if [ -d /run/current-system/share/oryx-bench/keyboards ]; then
  #       echo "Keyboards available:"
  #       ls /run/current-system/share/oryx-bench/keyboards/
  #     fi
  #   '';
  # };

  # ============================================================================
  # Rest of your system configuration
  # ============================================================================
  networking.hostName = "nixos";
  networking.networkmanager.enable = true;

  time.timeZone = "UTC";
  i18n.defaultLocale = "en_US.UTF-8";

  services.xserver.enable = true;
  services.xserver.displayManager.gdm.enable = true;
  services.xserver.desktopManager.gnome.enable = true;

  # Docker (needed for oryx-bench to build firmware)
  virtualisation.docker.enable = true;
  users.users.hybridz.extraGroups = [ "docker" ];

  # ... rest of your configuration
}
