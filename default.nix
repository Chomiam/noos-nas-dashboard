{ pkgs ? import <nixpkgs> {} }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "steveos-nas-dashboard";
  version = "0.2.100";

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
    mkdir -p $out/share/steveos-nas-dashboard
    cp -r frontend $out/share/steveos-nas-dashboard/
  '';

  meta = with pkgs.lib; {
    description = "Tableau de bord web en Rust & JS pour STEvE_OS NAS Edition (Catppuccin Mocha)";
    homepage = "https://github.com/Chomiam/steveos-nas-dashboard";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
