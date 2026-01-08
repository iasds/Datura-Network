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
        toolchain = fenix.packages.${system}.stable.toolchain;
        pkgs = nixpkgs.legacyPackages.${system};
        platform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
      in
      {
        packages = {
          default =

             platform.buildRustPackage
            {
              pname = "variable_pricing";
              nativeBuildInputs = with pkgs; [cmake ];
              buildInputs = with pkgs; [   stdenv.cc.cc.lib ];
              version = "0.1.0";

              src = ./.;

              cargoLock.lockFile = ./Cargo.lock;
            };
            doc = platform.buildRustPackage {
              name = "package-doc";
              dontCheck = true;
              dontInstall = true;
              nativeBuildInputs = with pkgs; [cmake ];
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
              (fenix.packages.${system}.stable.withComponents [
                "cargo"
                "clippy"
                "rust-src"
                "rustc"
                "rustfmt"
              ])
              pkgs.cmake
            ];

            shellHook = ''
              export CARGO_HOME="$PWD/.cargo"
              export PATH="$CARGO_HOME/bin:$PATH"
              export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib";
              mkdir -p .cargo
              echo '*' > .cargo/.gitignore
            '';
          };
        };
      }
    );
}
