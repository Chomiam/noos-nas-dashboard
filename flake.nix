{
  description = "STEvE_OS NAS Dashboard (Rust + Axum + Vanilla JS + Catppuccin Mocha)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.callPackage ./default.nix {};
        packages.steveos-nas-dashboard = self.packages.${system}.default;

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/steveos-nas-dashboard";
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
          ];
        };
      }
    ) // {
      # Module NixOS pour intégration directe dans STEvE_OS NAS Edition
      nixosModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.services.steveos-nas-dashboard;
          pkg = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        in {
          options.services.steveos-nas-dashboard = {
            enable = lib.mkEnableOption "Tableau de bord STEvE_OS NAS Edition";
            port = lib.mkOption {
              type = lib.types.port;
              default = 9339;
              description = "Port d'écoute du tableau de bord NAS";
            };
            openFirewall = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Ouvrir automatiquement le port dans le pare-feu modulaire";
            };
            user = lib.mkOption {
              type = lib.types.str;
              default = "chomiam";
              description = "Utilisateur non-root pour les commandes et les mises a jour";
            };
          };

          config = lib.mkIf cfg.enable {
            systemd.services.steveos-nas-dashboard = {
              description = "STEvE_OS NAS Dashboard Web Server";
              after = [ "network.target" ];
              wantedBy = [ "multi-user.target" ];
              stopIfChanged = false;
              path = with pkgs; [
                git
                gh
                nh
                nix
                nixos-rebuild
                nvd
                smartmontools
                cfspeedtest
                pciutils
                usbutils
                iproute2
                wireguard-tools
                qrencode
                coreutils
                whois openssl
                bash
                systemd
                diffutils
                gnugrep
                gnused
                findutils
                util-linux
                procps
                which
                mdadm
                btrfs-progs
                e2fsprogs
                xfsprogs
                dosfstools
                parted
                lvm2
                imagemagick
                exiftool
                libheif
                libraw
                ffmpeg
                yt-dlp
                libreoffice-still
                curl
                docker
                zip
                unzip
                p7zip
                gnutar
                gzip
                bzip2
                xz
                zstd
              ];
              environment = {
                STEVEOS_PORT = toString cfg.port;
                STEVEOS_FRONTEND_DIR = "${pkg}/share/steveos-nas-dashboard/frontend";
                STEVEOS_CONFIG_DIR = "/etc/nixos";
                STEVEOS_USER = cfg.user;
                NH_FLAKE = "/etc/nixos";
                NIX_CONFIG = "extra-experimental-features = nix-command flakes";
              };
              serviceConfig = {
                ExecStart = "${pkg}/bin/steveos-nas-dashboard";
                Restart = "always";
                RestartSec = "5s";
                DynamicUser = false;
                User = "root";
              };
            };

            environment.systemPackages = [ pkg ];
            networking.firewall.allowedTCPPorts = lib.optional cfg.openFirewall cfg.port;
          };
        };
    };
}
