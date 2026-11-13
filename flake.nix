{
  description = "saarthi - Guiding agents to victory (PRJ-0001)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    # A Rust toolchain with the Android targets' standard library, which nixpkgs' rustc
    # lacks; used by the android shell only (ADR-0024).
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    # The yajna CLI for the notebook under notes/ (github:leoank/yajna).
    yajna.url = "github:leoank/yajna";
    yajna.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];

      # The hub (NixOS) and node (home-manager) modules; this repo is their source of truth
      # (ADR-0041). Each defaults to this flake's package for the host's system.
      flake = let
        module = file: { _file = file; imports = [ (import file inputs.self) ]; };
      in {
        nixosModules.saarthi-hub = module ./nix/hub-module.nix;
        homeModules.saarthi-node = module ./nix/node-module.nix;
      };

      perSystem = { config, system, lib, ... }:
        let
          # The Android SDK is unfree: its licence is accepted here, and only its components
          # may be unfree, under the names androidenv gives them (ADR-0024).
          androidComponents = [ "build-tools" "cmake" "cmdline-tools" "emulator" "ndk"
                                "platform-tools" "platforms" "tools" ];
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
            config = {
              android_sdk.accept_license = true;
              allowUnfreePredicate = pkg:
                let n = lib.getName pkg; in
                lib.hasPrefix "android-sdk-" n || lib.hasPrefix "system-image-" n
                || builtins.elem n androidComponents;
            };
          };
          android = pkgs.androidenv.composeAndroidPackages {
            platformVersions = [ "35" ];
            buildToolsVersions = [ "35.0.0" ];
            includeNDK = true;
            includeEmulator = true;
            includeSystemImages = true;
            systemImageTypes = [ "google_apis" ];
            abiVersions = [ "arm64-v8a" ];
          };
          sdk = "${android.androidsdk}/libexec/android-sdk";
          rustAndroid = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "clippy" "rustfmt" ];
            targets = [ "aarch64-linux-android" ];
          };

          # libtailscale as a static archive for the tailnet crate (ADR-0015, ADR-0048).
          # `ac7ed43` marks the descriptors it hands out close-on-exec, so a spawned
          # session never inherits a tailnet connection ([[EXP-20261112-rq8kgc]]). Bump
          # the Go attribute with the rev when its go.mod moves.
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
            # A `package main` with cgo exports, built as a c-archive.
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

          # The same library for the phone's client core (ADR-0016): cgo through the NDK's
          # clang for arm64 Android at the app's minSdk. Go builds no c-archive for
          # Android, so it is a shared library the app ships beside the core
          # ([[EXP-20261113-1h4ztf]]).
          libtailscale-android = libtailscale.overrideAttrs (old: {
            pname = "libtailscale-android";
            # The app sandbox denies netlink, so the interfaces come from getifaddrs.
            postPatch = (old.postPatch or "") + ''
              cp ${./nix/libtailscale-android.go} android_interfaces.go
            '';
            env = old.env // {
              GOOS = "android";
              GOARCH = "arm64";
            };
            buildPhase = ''
              runHook preBuild
              # The stdenv sets CC to its own compiler and points compilers at the macOS
              # SDK; cgo needs the NDK's clang, which brings its own sysroot.
              unset SDKROOT DEVELOPER_DIR CPATH C_INCLUDE_PATH LIBRARY_PATH NIX_CFLAGS_COMPILE NIX_LDFLAGS
              export CC=${sdk}/ndk/${android.ndk-bundle.version}/toolchains/llvm/prebuilt/${if pkgs.stdenv.buildPlatform.isDarwin then "darwin" else "linux"}-x86_64/bin/aarch64-linux-android26-clang
              go build -buildvcs=false -buildmode=c-shared -ldflags='-s -w -extldflags=-Wl,-soname,libtailscale.so' -o libtailscale.so .
              runHook postBuild
            '';
            installPhase = ''
              runHook preInstall
              install -Dm644 libtailscale.so $out/lib/libtailscale.so
              install -Dm644 tailscale.h $out/include/tailscale.h
              runHook postInstall
            '';
            # An Android ELF: the host's strip and patchelf have no business with it.
            dontFixup = true;
          });

          # The workspace source, shared by the checks and the binary packages:
          # spikes/, docs/ and android/ stay out of it.
          saarthiSrc = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./crates ];
          };
          # One release binary of the workspace. `bin` is the binary name; the
          # node's `saarthi` and the hub's `saarthi-hub` are each built on their own so
          # the closure holds only that binary's dependency graph (the uniffi core
          # and bindgen crates never enter the hub or node). Tests are skipped: the
          # hub's suite spins up a node and the node's needs a PTY; the workspace
          # check already builds and tests everything.
          mkBin = pname: bin: pkgs.rustPlatform.buildRustPackage {
            inherit pname;
            version = "0.5.0";
            src = saarthiSrc;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.protobuf ];
            # sqlx's query macros link the system sqlite on the build host (the hub's
            # own copy is bundled); the macOS SDK stopped providing it in nixpkgs.
            buildInputs = [ pkgs.sqlite ];
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            LIBTAILSCALE_LIB_DIR = "${libtailscale}/lib";
            cargoBuildFlags = [ "--bin" bin ];
            doCheck = false;
            meta.mainProgram = bin;
          };
        in
        {
          _module.args.pkgs = pkgs;

          # The deployable binaries, the defaults of the hub and node modules.
          packages = rec {
            inherit libtailscale libtailscale-android;
            saarthi-hub = mkBin "saarthi-hub" "saarthi-hub";
            saarthi = mkBin "saarthi" "saarthi";
            # The user guide for GitHub Pages: mdBook, with the command-line reference and
            # the design section generated by docs/guide/build.py.
            docs = pkgs.runCommand "saarthi-docs" {
              nativeBuildInputs = [ saarthi saarthi-hub pkgs.mdbook pkgs.python3 ];
            } ''
              python3 ${./docs/guide/build.py} ${lib.fileset.toSource {
                root = ./.;
                fileset = lib.fileset.unions [ ./docs ./notes ];
              }} $out
            '';
          };

          devShells.default = pkgs.mkShell {
            packages = [
              pkgs.python3
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt
              pkgs.rust-analyzer
              pkgs.protobuf
              pkgs.sqlite
              inputs.yajna.packages.${system}.yajna
            ];
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            LIBTAILSCALE_LIB_DIR = "${libtailscale}/lib";
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };

          # The Android app and the client core for Android: the SDK with an arm64 emulator
          # image, the NDK, cargo-ndk, Gradle and a JDK. Several GB, so it is a shell of its
          # own and the default shell and the checks stay light.
          devShells.android = pkgs.mkShell {
            packages = [
              rustAndroid
              pkgs.cargo-ndk
              pkgs.protobuf
              pkgs.gradle
              pkgs.jdk17
              android.androidsdk
              # A local ntfy for push in development (ADR-0025).
              pkgs.ntfy-sh
            ];
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            # The core links libtailscale for the phone; build-core.sh copies it into the
            # app's jniLibs. The host's archive is for the bindings generator's host build.
            LIBTAILSCALE_ANDROID_LIB_DIR = "${libtailscale-android}/lib";
            LIBTAILSCALE_LIB_DIR = "${libtailscale}/lib";
            ANDROID_HOME = sdk;
            ANDROID_SDK_ROOT = sdk;
            ANDROID_NDK_HOME = "${sdk}/ndk/${android.ndk-bundle.version}";
            JAVA_HOME = pkgs.jdk17.home;
            # Emulator devices and Android tool state live outside the repo and the store.
            shellHook = ''
              export ANDROID_USER_HOME="''${XDG_STATE_HOME:-$HOME/.local/state}/saarthi/android"
              export ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd"
              export GRADLE_USER_HOME="''${XDG_STATE_HOME:-$HOME/.local/state}/saarthi/gradle"
              mkdir -p "$ANDROID_AVD_HOME"
            '';
          };

          # The notebook passes yajna validate.
          checks.notebook = pkgs.runCommand "saarthi-notebook" {
            nativeBuildInputs = [ inputs.yajna.packages.${system}.yajna ];
          } ''
            cd ${lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [ ./yajna.toml ./notes ];
            }}
            yajna --root . validate
            touch $out
          '';

          # The modules evaluate to the services and flags they promise.
          checks.modules = import ./nix/module-tests.nix {
            inherit pkgs lib;
            inherit (inputs) nixpkgs self;
          };

          # The guide builds and every relative link in it resolves.
          checks.docs = config.packages.docs;

          # The cargo workspace, built and tested; spikes/ and docs/ stay out of the source.
          checks.workspace = pkgs.rustPlatform.buildRustPackage {
            pname = "saarthi";
            version = "0.5.0";
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./crates ];
            };
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.protobuf ];
            buildInputs = [ pkgs.sqlite ];
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            LIBTAILSCALE_LIB_DIR = "${libtailscale}/lib";
          };
        };
    };
}
