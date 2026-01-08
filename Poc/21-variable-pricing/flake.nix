{
  inputs = {
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixos-unstable";
  };

  outputs =
    {
      self,
      fenix,
      flake-utils,
      nixpkgs,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:

      let
        toolchain = fenix.packages.${system}.complete.toolchain;
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages = {
          default =

          pkgs.rustPlatform.buildRustPackage
            {
              pname = "package";
              version = "0.1.0";

              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;
            };
            doc = pkgs.rustPlatform.buildRustPackage {
              name = "package-doc";
              dontCheck = true;
              dontInstall = true;
              cargoLock.lockFile = ./Cargo.lock;
              src = ./.;
              buildPhase=  ''
                mkdir -p $out
                cargo doc --offline
                cp -a target/doc $out/'';
            };
          };
        devShells = {
          default = pkgs.mkShell {
            buildInputs = [
              (fenix.packages.${system}.default.withComponents [
                "cargo"
                "clippy"
                "rust-src"
                "rustc"
                "rustfmt"
              ])
            ];

            shellHook = ''
              export CARGO_HOME="$PWD/.cargo"
              export PATH="$CARGO_HOME/bin:$PATH"
              mkdir -p .cargo
              echo '*' > .cargo/.gitignore
            '';
          };
        };
      }
    );
}
