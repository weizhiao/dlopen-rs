cargo check -p dlopen-rs  --no-default-features --features=""
cargo check -p dlopen-rs  --no-default-features --features="std"
cargo check -p dlopen-rs  --no-default-features --features="tls"
# 检查unwind相关的feature
cargo check -p dlopen-rs  --no-default-features --features="libgcc"
cargo check -p dlopen-rs  --no-default-features --features="libunwind"
cargo check -p dlopen-rs  --no-default-features --features="unwinding"
# 检查其余的feature
cargo check -p dlopen-rs  --no-default-features --features="debug"
cargo check -p dlopen-rs  --no-default-features --features="version"
# 检查常规组合
cargo check -p dlopen-rs  --no-default-features --features="mmap,libgcc,tls,debug"
cargo check -p dlopen-rs  --no-default-features --features="libgcc,tls,debug,version"