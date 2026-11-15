# EXP-20261114-raqjf0: does libghostty-vt build for Android arm64 under Nix?
# Throwaway spike; standalone flake on the project flake's nixpkgs rev. Ghostty's source
# and its Zig dependency cache come from nixpkgs' ghostty package (v1.4.0, Zig 0.17); the
# NDK from androidenv, as in the project's android shell (ADR-0024).
{
  description = "EXP-20261114-raqjf0 libghostty-vt for Android";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/f4a03c008967841f2a0b3cb7c436fc6fc31671f6";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {
        inherit system;
        config = {
          android_sdk.accept_license = true;
          allowUnfreePredicate = pkg:
            let n = nixpkgs.lib.getName pkg; in
            nixpkgs.lib.hasPrefix "android-sdk-" n
            || builtins.elem n [ "ndk" "platform-tools" "build-tools" "cmdline-tools" "tools" "platforms" ];
        };
      }));
    in
    {
      packages = forAll (pkgs:
        let
          ghostty = pkgs.ghostty;
          # Only the NDK: the composed SDK would fetch the emulator as well.
          android = pkgs.androidenv.composeAndroidPackages {
            includeNDK = true;
            includeEmulator = false;
            includeSystemImages = false;
          };
          ndk = "${android.ndk-bundle}/libexec/android-sdk/ndk/${android.ndk-bundle.version}";
          # zig build -Demit-lib-vt, as libghostty-vt-sys and Ghostty's PACKAGING.md do.
          libvt = { name, target ? null, env ? "" }: pkgs.stdenv.mkDerivation {
            pname = "libghostty-vt-${name}";
            inherit (ghostty) version src;
            # On macOS the static library is combined with Apple's libtool (cctools).
            nativeBuildInputs = [ pkgs.zig_0_17 ]
              ++ pkgs.lib.optional (target == null && pkgs.stdenv.hostPlatform.isDarwin) pkgs.cctools;
            dontConfigure = true;
            dontSetZigDefaultFlags = true;
            dontFixup = target != null;
            buildPhase = ''
              runHook preBuild
              export ZIG_GLOBAL_CACHE_DIR=$TMPDIR/zig-global
              ${env}
              zig build \
                --system ${ghostty.deps} \
                --cache-dir $TMPDIR/zig-cache \
                --prefix $out \
                -Demit-lib-vt=true \
                -Dapp-runtime=none \
                -Demit-xcframework=false \
                -Dversion-string=${ghostty.version} \
                -Doptimize=ReleaseFast \
                ${if target == null then "-Dcpu=baseline" else "-Dtarget=${target}"}
              runHook postBuild
            '';
            installPhase = "true";
          };
        in
        {
          # The control: the same library for the machine building it.
          host = libvt { name = "host"; };
          # API level 26, the app's minSdk.
          android-arm64 = libvt {
            name = "android-arm64";
            target = "aarch64-linux-android.26";
            env = "export ANDROID_NDK_HOME=${ndk}";
          };
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.android-arm64;
        });
    };
}
