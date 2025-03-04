with import ./nix/pkgs.nix {
  overlays = [ (import ./nix/overlay.nix) ];
};
let merged-openssl = symlinkJoin { name = "merged-openssl"; paths = [ openssl.out openssl.dev ]; };
in stdenv.mkDerivation rec {
  name = "rust-env";
  env = buildEnv { name = name; paths = buildInputs; };

  buildInputs = [
    rustup
    openssl
  ];
  shellHook = ''
    export OPENSSL_DIR="${merged-openssl}" 
  '';
}
