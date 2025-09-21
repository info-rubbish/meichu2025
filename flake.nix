{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
      in
      {
        defaultPackage = naersk-lib.buildPackage ./.;
        devShell = with pkgs; mkShell rec {
          buildInputs = [
            cargo
            rustc
            rustPackages.clippy
            pnpm
            pkg-config
            openssl
            nodejs-slim
            libxkbcommon
            libGL
            # WINIT_UNIX_BACKEND=wayland
            wayland
          ];
          nativeBuildInputs = [
            rustfmt
            just
            nushell
            sea-orm-cli
            typeshare
          ];
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
          LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
        };
      }
    );
}
