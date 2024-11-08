cargo check -p dlopen-rs  --no-default-features --features=""
cargo check -p dlopen-rs  --no-default-features --features="std"
cargo check -p dlopen-rs  --no-default-features --features="tls"
# 检查unwind相关的feature
cargo check -p dlopen-rs  --no-default-features --features="libgcc"
cargo check -p dlopen-rs  --no-default-features --features="libunwind"
cargo check -p dlopen-rs  --no-default-features --features="unwinding"
# 检查自带的Mmap trait实现
cargo check -p dlopen-rs  --no-default-features --features="mmap_impl"
cargo check -p dlopen-rs  --no-default-features --features="no_mmap_impl"
cargo check -p dlopen-rs  --no-default-features --features="no_mmap_impl,std"
# 检查其余的feature
cargo check -p dlopen-rs  --no-default-features --features="ldso"
cargo check -p dlopen-rs  --no-default-features --features="debug"
cargo check -p dlopen-rs  --no-default-features --features="version"
# 检查常规组合
cargo check -p dlopen-rs  --no-default-features --features="mmap_impl,libgcc,tls,debug"
cargo check -p dlopen-rs  --no-default-features --features="no_mmap_impl,libgcc,tls,debug,version"
cargo check -p dlopen-rs  --no-default-features --features="libgcc,tls,debug,version"