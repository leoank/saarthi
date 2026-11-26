# EXP-20261126-f3e5a9: the Cedar spike as a package for each of the hub's targets, its
# checkPhase running the case table there. Throwaway; the project flake's nixpkgs rev.
{
  description = "EXP-20261126-f3e5a9 Cedar spike";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/f4a03c008967841f2a0b3cb7c436fc6fc31671f6";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "aarch64-linux" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "cedar-spike";
          version = "0.1.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./src ./policies ./saarthi.cedarschema ];
          };
          cargoLock.lockFile = ./Cargo.lock;
        };
      });
    };
}
