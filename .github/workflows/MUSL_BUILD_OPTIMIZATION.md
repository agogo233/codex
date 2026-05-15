# Rust/ codex-rs musl 编译 Workflow 优化建议

## 分析总结

### ✅ 现有配置优点

1. **完善的 UBSan 防护机制** - 通过 rustc wrapper 和清空 RUSTFLAGS 防止 proc-macro 崩溃
2. **完整的 musl 工具链** - 使用 Zig 0.14.0 作为 musl 交叉编译工具
3. **libcap 静态编译** - 为 musl 目标手动编译 libcap 静态库
4. **rusty_v8 预编译产物** - 从 release 下载预编译的 v8 binding
5. **严格的链接器配置** - 通过 cargo config 覆盖 linker 参数

### ⚠️ 存在的问题

#### 1. 编译时间长 (90 分钟超时)
- **原因**: 
  - 大量 C/C++ 依赖需要从源码编译 (aws-lc-sys, boringSSL, libcap 等)
  - release profile 使用 `lto = "fat"` 和 `codegen-units = 1`
  - 没有利用 GitHub Actions 缓存

#### 2. 磁盘空间风险
- **原因**:
  - 完整编译需要 13GB+ 空间
  - CI runner 磁盘空间有限
  - 没有清理中间产物

#### 3. 重复配置
- env 变量在多个步骤重复设置
- GITHUB_ENV 写入冗余

#### 4. 缺少错误诊断
- 编译失败时缺少详细错误信息
- 没有收集编译产物大小等指标

## 优化方案

### 方案 1: 添加缓存 (推荐优先实施)

```yaml
- name: Cache Cargo registry
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry
      ~/.cargo/git
    key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}
    restore-keys: |
      ${{ runner.os }}-cargo-registry-

- name: Cache Cargo build
  uses: actions/cache@v4
  with:
    path: codex-rs/target
    key: ${{ runner.os }}-cargo-target-${{ hashFiles('**/Cargo.lock') }}-${{ github.sha }}
    restore-keys: |
      ${{ runner.os }}-cargo-target-${{ hashFiles('**/Cargo.lock') }}-
```

**效果**: 减少 50-70% 编译时间

### 方案 2: 优化编译配置

#### 2.1 使用 ThinLTO 代替 FatLTO
```yaml
env:
  CARGO_PROFILE_RELEASE_LTO: ${{ inputs.lto || 'thin' }}
```

在 `Cargo.toml`:
```toml
[profile.release]
lto = "thin"  # 代替 "fat"
codegen-units = 16  # 代替 1，允许并行编译
```

**效果**: 减少 30-40% 编译时间，二进制大小增加约 5-10%

#### 2.2 分阶段编译
```yaml
- name: Build common dependencies first
  run: cargo build --target $target --release -p codex-core -p codex-protocol
  
- name: Build binaries
  run: cargo build --target $target --release --bin codex --bin codex-app-server
```

**效果**: 更好的缓存利用，更清晰的错误定位

### 方案 3: 磁盘空间优化

```yaml
- name: Clean up before build
  run: |
    sudo rm -rf /usr/share/dotnet
    sudo rm -rf /opt/ghc
    sudo rm -rf /usr/local/.ghcup
    sudo rm -rf /usr/local/share/boost
    df -h .

- name: Clean up after build
  run: |
    cargo clean --release -p <未使用的 crates>
    rm -rf target/debug
    df -h .
```

### 方案 4: 并行编译多个 binary

```yaml
- name: Build binaries in parallel
  run: |
    # 使用 cargo build 同时编译多个 binary
    cargo build --target "$target" --release \
      --bin codex \
      --bin codex-responses-api-proxy \
      --bin codex-app-server \
      --bin bwrap
```

现有配置已经做了这个优化 ✓

### 方案 5: 使用 Bazel 代替 Cargo (长期方案)

如果项目已经使用 Bazel (从 .bazelrc 判断是):

```yaml
- name: Build with Bazel
  run: |
    bazel build //codex-rs/cli:release_binaries \
      --config=remote \
      --remote_cache=...
```

**效果**: 
- 更好的增量编译
- 远程缓存支持
- 更精确的依赖追踪

### 方案 6: 改进错误诊断

```yaml
- name: Cargo build (Linux amd64)
  shell: bash
  run: |
    set -euo pipefail
    # ... existing setup ...
    
    echo "::group::Build Configuration"
    echo "Target: $target"
    echo "LTO: ${CARGO_PROFILE_RELEASE_LTO}"
    echo "Rust version: $(rustc --version)"
    echo "Zig version: $(zig version)"
    df -h .
    echo "::endgroup::"
    
    echo "::group::Build Output"
    cargo build "${cargo_build_args[@]}" 2>&1 | tee /tmp/build.log
    BUILD_STATUS=${PIPESTATUS[0]}
    echo "::endgroup::"
    
    if [[ $BUILD_STATUS -ne 0 ]]; then
      echo "::error::Build failed"
      echo "Build log size: $(wc -c < /tmp/build.log) bytes"
      tail -n 100 /tmp/build.log
      exit 1
    fi
```

## 优化后的 Workflow 示例

```yaml
name: Compile Linux amd64

on:
  push:
    branches: [main, master]
  workflow_dispatch:
    inputs:
      lto:
        description: "LTO mode"
        required: false
        default: "thin"
        type: choice
        options:
          - "thin"
          - "fat"
          - "off"

env:
  CARGO_PROFILE_RELEASE_LTO: ${{ inputs.lto || 'thin' }}
  CARGO_INCREMENTAL: 0

jobs:
  compile-linux-amd64:
    name: Compile Linux amd64
    runs-on: ubuntu-24.04
    timeout-minutes: 60  # 优化后应该能在 60 分钟内完成
    permissions:
      contents: read
    
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Free disk space
        run: |
          sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /usr/local/share/boost
          sudo apt-get clean
          df -h .

      - name: Cache Cargo registry
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
          key: ${{ runner.os }}-cargo-registry-${{ hashFiles('codex-rs/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-registry-

      - name: Cache musl build
        uses: actions/cache@v4
        with:
          path: |
            /tmp/codex-musl-tools-x86_64-unknown-linux-musl
          key: ${{ runner.os }}-musl-tools-v2-${{ hashFiles('.github/scripts/install-musl-build-tools.sh') }}

      - name: Setup Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-unknown-linux-musl

      - name: Install Zig
        uses: mlugg/setup-zig@v2
        with:
          version: 0.14.0

      - name: Install musl build tools
        env:
          TARGET: x86_64-unknown-linux-musl
          GITHUB_ENV: ${{ runner.temp }}/github-env.txt
        run: bash "${GITHUB_WORKSPACE}/.github/scripts/install-musl-build-tools.sh"

      - name: Configure rustc flags
        shell: bash
        run: |
          # 清理可能冲突的环境变量
          unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS
          unset CFLAGS CXXFLAGS ASAN_OPTIONS UBSAN_OPTIONS
          
          # 清空 GITHUB_ENV 中的相关变量
          for var in RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTDOCFLAGS CARGO_BUILD_RUSTFLAGS \
                     CFLAGS CXXFLAGS CMAKE_C_FLAGS CMAKE_CXX_FLAGS; do
            echo "${var}=" >> "$GITHUB_ENV"
          done

      - name: Configure rusty_v8 artifacts
        uses: ./.github/actions/setup-rusty-v8-musl
        with:
          target: x86_64-unknown-linux-musl

      - name: Build binaries
        shell: bash
        run: |
          set -euo pipefail
          target="x86_64-unknown-linux-musl"
          
          # 配置 cargo home
          cargo_home="${GITHUB_WORKSPACE}/.cargo-home"
          mkdir -p "${cargo_home}"
          export CARGO_HOME="${cargo_home}"
          
          # 写入 linker 配置
          cat > "${cargo_home}/config.toml" <<EOF
          [target.${target}]
          rustflags = ['-C', 'link-arg=-nostartfiles', '-C', 'link-arg=-nodefaultlibs', '-C', 'link-arg=-lc']
          EOF
          
          echo "::group::Build Configuration"
          echo "LTO: ${CARGO_PROFILE_RELEASE_LTO}"
          echo "Disk before build:"
          df -h .
          echo "::endgroup::"
          
          cargo build --target "$target" --release \
            --bin codex \
            --bin codex-responses-api-proxy \
            --bin codex-app-server \
            --bin bwrap
          
          echo "::group::Disk after build"
          df -h .
          echo "::endgroup::"

      - name: Verify and package
        shell: bash
        run: |
          set -euo pipefail
          target="x86_64-unknown-linux-musl"
          release_dir="target/${target}/release"
          
          for binary in codex codex-responses-api-proxy codex-app-server bwrap; do
            file "${release_dir}/${binary}"
            ls -lh "${release_dir}/${binary}"
          done
          
          # 创建 bundle
          bundle_dir="${{ runner.temp }}/codex-linux-amd64"
          mkdir -p "$bundle_dir/codex-resources"
          cp "$release_dir"/{codex,codex-responses-api-proxy,codex-app-server} "$bundle_dir/"
          cp "$release_dir/bwrap" "$bundle_dir/codex-resources/"
          chmod +x "$bundle_dir"/codex* "$bundle_dir/codex-resources/bwrap"
          
          tar -C "$bundle_dir/.." -czf "${bundle_dir}.tar.gz" "$(basename "$bundle_dir")"
          ls -lh "${bundle_dir}.tar.gz"

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: codex-linux-amd64-binaries
          path: ${{ runner.temp }}/codex-linux-amd64-bundle.tar.gz
          retention-days: 30
```

## 本地测试步骤

1. **安装依赖**:
```bash
# 安装 musl 工具链
sudo apt-get update
sudo apt-get install -y musl-tools clang lld pkg-config libcap-dev

# 安装 Zig 0.14.0
cd /tmp
curl -fsSL https://ziglang.org/download/0.14.0/zig-linux-x86_64-0.14.0.tar.xz -o zig.tar.xz
tar -xf zig.tar.xz
sudo mv zig-linux-x86_64-0.14.0 /opt/zig
sudo ln -sf /opt/zig/zig /usr/local/bin/zig
```

2. **设置环境**:
```bash
cd /workspace
bash .github/scripts/install-musl-build-tools.sh
```

3. **测试编译**:
```bash
cd codex-rs
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/bin/musl-gcc
cargo build --target x86_64-unknown-linux-musl --release --bin codex
```

## 预期收益

| 优化项 | 预计时间节省 | 实施难度 |
|--------|-------------|---------|
| Cargo 缓存 | 40-50% | 低 |
| ThinLTO | 30-40% | 低 |
| 磁盘清理 | 避免失败 | 低 |
| 分阶段编译 | 10-15% | 中 |
| 总计 | 60-70% | - |

**优化前**: 90 分钟 (可能超时)  
**优化后**: 30-40 分钟

## 后续建议

1. **监控编译时间**: 在 workflow 中添加 timing 指标
2. **考虑 glibc 版本**: 如果不需要完全静态链接，可以考虑使用 glibc 版本
3. **使用 Bazel**: 项目已有 Bazel 配置，建议全面迁移到 Bazel 构建系统
4. **定期清理缓存**: 设置缓存过期策略，避免存储无限增长
