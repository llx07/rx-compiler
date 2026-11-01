# Fill in these five commands. See README-EN.md or README-ZH.md for details.
CLANG ?= clang-22
# Optional: build your compiler once before testing. Leave empty if prebuilt.
# {runtime} will be linked into every program.
# Write shared runtime assembly there if your IR/CODEGEN output references it.
BUILD = cargo build --release && \
    $(CLANG) --target=riscv32-unknown-elf -march=rv32im -mabi=ilp32 \
    -O2 -fno-builtin -S runtime/runtime.c -o {runtime}

ifeq ($(shell uname -s),Darwin)
CLANG = /opt/homebrew/opt/llvm@22/bin/clang
endif

# Required for semantic tests: exit 0 to accept {source}, 1 to reject it.
SEMANTIC = ./target/release/rx-compiler --stage=semantic {source}

# Required for IR tests: write RV32 LLVM IR to {output} (.ll).
IR = ./target/release/rx-compiler --stage=ir {source} -o {output}

# Required for codegen/optimization tests: write RV32IM assembly to {output} (.s).
CODEGEN = ./target/release/rx-compiler --stage=ir {source} -o {output}.ll && \
    $(CLANG) -S -x ir --target=riscv32-unknown-elf -march=rv32im -mabi=ilp32 \
    -O2 -mllvm -riscv-no-aliases {output}.ll -o {output}

# Required alongside IR/CODEGEN: run the assembly with the BUILD runtime in REIMU.
RUN = xmake run -P vendor/REIMU reimu --memory=256M --stack=1M \
    -f {output},{runtime} -o {stdout} -p {profile} 1>&2
