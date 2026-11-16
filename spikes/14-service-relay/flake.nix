# EXP-20261114-raqjjy: the spike relay as a package, so it can run on another machine
# (the hub host) without a toolchain there. Throwaway; the project flake's nixpkgs rev.
{
  description = "EXP-20261114-raqjjy service relay spike";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/f4a03c008967841f2a0b3cb7c436fc6fc31671f6";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "aarch64-linux" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "service-relay";
          version = "0.1.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./build.rs ./src ./proto ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.protobuf ];
          PROTOC = "${pkgs.protobuf}/bin/protoc";
        };
      });
    };
}
