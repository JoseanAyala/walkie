# NixOS module: installs walkie and grants the access it needs to read the
# keyboard (the evdev hook in hotkey/tap/linux.rs) and open a uinput device
# (inject/chord/linux.rs, which presses Ctrl+V for pastes).
# Launch-at-login is handled by the app itself via an XDG
# autostart .desktop file (login_item/linux.rs), so there's no systemd
# service here.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.walkie;
in
{
  options.programs.walkie = {
    enable = lib.mkEnableOption "walkie, a local-only dictation hotkey";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.walkie;
      defaultText = lib.literalExpression "pkgs.walkie";
      description = ''
        The walkie package to install. The default (`pkgs.walkie`) only
        resolves if this flake's `overlays.default` is in your
        `nixpkgs.overlays`; otherwise set this explicitly, e.g.
        `walkie.packages.${pkgs.system}.walkie` from your flake inputs.
      '';
    };

    users = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "alice" ];
      description = ''
        Usernames to add to the `input` and `uinput` groups, so walkie can
        read the keyboard (its hotkey hook) and open `/dev/uinput` (to
        synthesize the Ctrl+V it pastes with). Without this, walkie runs
        but can't see key events or paste.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    # Creates the `uinput` group, loads the kernel module, and sets up the
    # udev rule making /dev/uinput group-writable.
    hardware.uinput.enable = true;

    users.users = lib.genAttrs cfg.users (_: {
      extraGroups = [
        "input"
        "uinput"
      ];
    });
  };
}
