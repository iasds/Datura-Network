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
              pname = "xmr-pow-challenges";
              nativeBuildInputs = with pkgs; [cmake ];
              buildInputs = with pkgs; [   stdenv.cc.cc.lib ];
              version = "0.1.0";

              src = ./.;

              cargoLock.lockFile = ./Cargo.lock;
            };
            doc = pkgs.stdenv.mkDerivation {
              name = "pow-challenge-doc";
              buildInputs=  with pkgs; [cargo];
              buildPhase=  ''
                mkdir $out/doc
                cd $src
                cargo doc -o $out/doc'';
            };
          };
        devShells = {
          default = pkgs.mkShell {
            buildInputs = [
              (fenix.packages.${system}.complete.withComponents [
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
              mkdir -p .cargo
              echo '*' > .cargo/.gitignore
            '';
          };
        };
      }
    );
}
