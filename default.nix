{ pkgs ? import <nixpkgs> {} }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "noos-nas-dashboard";
  version = "0.3.3";

  src = ./.;

  buildInputs = [
    pkgs.libxcrypt
    pkgs.openssl
    pkgs.whois
  ];

  doCheck = false;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  postInstall = ''
    mkdir -p $out/share/noos-nas-dashboard
    cp -r frontend $out/share/noos-nas-dashboard/

    # Shims de rétrocompatibilité absolue STEvE_OS -> Noos
    mkdir -p $out/share/steveos-nas-dashboard
    ln -s $out/share/noos-nas-dashboard/frontend $out/share/steveos-nas-dashboard/frontend
    ln -s $out/bin/noos-nas-dashboard $out/bin/steveos-nas-dashboard
  '';

  meta = with pkgs.lib; {
    description = "Tableau de bord web en Rust & JS pour Noos NAS Edition (Catppuccin Mocha)";
    homepage = "https://github.com/Chomiam/noos-nas-dashboard";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
