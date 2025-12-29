{
  description = "A devShell example";

  inputs = {
    nixpkgs.url      = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        my-python-packages = ps: with ps; [
          sphinx
          sphinx_rtd_theme
          # other python packages
        ];
        R-with-my-packages = pkgs.rWrapper.override{ packages = with pkgs.rPackages; [ ggplot2 readr dplyr plotly mosaic DBI tikzDevice colorspace heatmaply RColorBrewer RSQLite languageserver ]; };
        clang-all = pkgs.symlinkJoin {
          name ="clang-all";
          paths = (with pkgs; [ llvmPackages_19.libclang.out llvmPackages_19.libllvm.out llvmPackages_19.libunwind.out
          llvmPackages_19.libclang.lib llvmPackages_19.libllvm.lib llvmPackages_19.libunwind
          llvmPackages_19.libclang.dev llvmPackages_19.libllvm.dev llvmPackages_19.libunwind.dev ]);
        };
      in
      with pkgs;
      rec {
        devShell = mkShell.override {stdenv = stdenv;} {  # LibAFL needs LLVM
          buildInputs = [
            (rust-bin.stable."1.87.0".default.override {
              extensions = [ "rust-src" "rust-analyzer" ];
            })
            cargo-make
            llvmPackages_19.clangUseLLVM
            clang-all
            zlib
            cargo-flamegraph
            # für qemu
            (pkgs.python3.withPackages my-python-packages)
            meson # unsure
            ninja
            pkg-config
            glib
            pixman
            # libslirp
            # für analyse der in-/outputs
            xxd
            # FreeRTOS
            gcc-arm-embedded
            openssl_legacy  # gdb needs libcrypt.so.1
            # generate bindings from RTOS to Rust
            rust-bindgen
            # Debugging
            ddd
            # visualization
            graphviz
            #rstudioWrapper # prefer host packages for R
            #R 
            R-with-my-packages
            pandoc
            # dependencies for mosaic
            freetype
            fontconfig
            # benchmarking
            snakemake
            vim
            psmisc
            sqlite
            heaptrack
          ];

          shellHook = ''
          export INSIDE_DEVSHELL=1
          export PATH=$PATH:$(pwd)/LibAFL/fuzzers/FRET/tools/bin
          export CUSTOM_QEMU_DIR=$(pwd)/qemu-libafl-bridge
          export LIBAFL_QEMU_DIR=$(pwd)/qemu-libafl-bridge
          export CUSTOM_QEMU_NO_BUILD=1
          export CUSTOM_QEMU_NO_CONFIGURE=1
          export LIBAFL_EDGES_MAP_SIZE_IN_USE=1048576
          # export EMULATION_MODE=systemmode
          # export CPU_TARGET=arm
          # export CROSS_CC=arm-none-eabi-gcc
          export LIBCLANG_PATH=${clang-all}/lib
          export BENCHDIR=bench_default

          export FREERTOS_KERNEL_PATH=$(pwd)/FreeRTOS-Kernel
          mkdir -p $TMPDIR
          '';
        };
      }
    );
}
