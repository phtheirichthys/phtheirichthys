{
  description = "phtheirichthys - sailing router";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = with pkgs; [
            openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];
        };

        # Build only the native binary (no wasm features)
        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          cargoExtraArgs = "--bin phtheirichthys --no-default-features --features tokio";
        });

        phtheirichthys = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin phtheirichthys --no-default-features --features tokio";
        });

      in
      {
        packages = {
          default = phtheirichthys;
          inherit phtheirichthys;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            wasm-pack
            wasm-bindgen-cli
            cargo-watch
            cargo-flamegraph
          ];

          shellHook = ''
            export RUST_LOG=info
            export OPENSSL_NO_VENDOR=1
          '';
        };

        checks = {
          inherit phtheirichthys;

          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--bin phtheirichthys --no-default-features --features tokio -- --deny warnings";
          });

          fmt = craneLib.cargoFmt { inherit src; };
        };
      });
}
