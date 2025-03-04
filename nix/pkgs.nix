# To update nix-prefetch-git https://github.com/NixOS/nixpkgs
import ((import <nixpkgs> {}).fetchFromGitHub {
  owner = "NixOS";
  repo = "nixpkgs";
  rev = "e4ee61d0f47e18e168b84985d5dcd0f1d9e3b60c";
  sha256  = "19hi097bcq5wd4y1ymj2z3zfl46xy73vkfrhg5z40azc2c88dlv6";
})
