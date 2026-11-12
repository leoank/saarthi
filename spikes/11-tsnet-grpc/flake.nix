# EXP-20261112-rq8kgc: tonic gRPC over libtailscale's listen and dial on a real tailnet.
# Throwaway spike; standalone flake on the project flake's nixpkgs rev.
{
  description = "EXP-20261112-rq8kgc tonic over libtailscale";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/f4a03c008967841f2a0b3cb7c436fc6fc31671f6";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        # ac7ed43 (2026-10-30) marks the descriptors it hands out close-on-exec.
        libtailscale = pkgs.buildGo127Module {
          pname = "libtailscale";
          version = "0-unstable-2026-10-30";
          src = pkgs.fetchFromGitHub {
            owner = "tailscale";
            repo = "libtailscale";
            rev = "ac7ed43af14e19e1c1810ee9f8319872c6beb412";
            hash = "sha256-K8QGeCyKBcYqBt9O192QpQ4EFRjgiPPE5djD+jyoM3M=";
          };
          vendorHash = "sha256-1Y21dNgCNMfVQvZN6k66GHvgdZo8lle5J8B5UES2Ryg=";
          env.CGO_ENABLED = "1";
          buildPhase = ''
            runHook preBuild
            go build -buildvcs=false -buildmode=c-archive -o libtailscale.a .
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            install -Dm644 libtailscale.a $out/lib/libtailscale.a
            install -Dm644 tailscale.h $out/include/tailscale.h
            runHook postInstall
          '';
          doCheck = false;
        };

        default = pkgs.rustPlatform.buildRustPackage {
          pname = "tsnet-grpc";
          version = "0.1.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./build.rs ./src ./proto ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.protobuf ];
          env.LIBTAILSCALE_LIB_DIR = "${libtailscale}/lib";
          doCheck = false;
        };
      });
    };
}
