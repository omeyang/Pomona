#!/bin/sh
# Pomona 一键安装：识别平台，下载最新 release 的 git-pomona 放入 PATH。
#   curl -fsSL https://raw.githubusercontent.com/omeyang/Pomona/main/install.sh | sh
# 可选环境变量：
#   POMONA_VERSION      指定版本（默认最新，如 v0.1.0）
#   POMONA_INSTALL_DIR  安装目录（默认 ~/.local/bin，root 为 /usr/local/bin）
set -eu

REPO="omeyang/Pomona"
BIN="git-pomona"

err() { printf 'install: %s\n' "$1" >&2; exit 1; }

os=$(uname -s)
arch=$(uname -m)
case "$os-$arch" in
  Linux-x86_64)              asset="$BIN-linux-x86_64" ;;
  Linux-aarch64|Linux-arm64) asset="$BIN-linux-aarch64" ;;
  *) err "暂不支持的平台: $os $arch（可用 cargo install --git https://github.com/$REPO 自行构建）" ;;
esac

if [ -n "${POMONA_VERSION:-}" ]; then
  version=$POMONA_VERSION
else
  resp=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest") || err "获取最新版本失败"
  version=$(printf '%s\n' "$resp" | grep -m1 '"tag_name"' | cut -d'"' -f4)
fi
[ -n "$version" ] || err "无法确定版本号"

if [ -n "${POMONA_INSTALL_DIR:-}" ]; then
  dir=$POMONA_INSTALL_DIR
elif [ "$(id -u)" = 0 ]; then
  dir=/usr/local/bin
else
  dir=$HOME/.local/bin
fi
mkdir -p "$dir"

base="https://github.com/$REPO/releases/download/$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

printf '下载 %s/%s ...\n' "$base" "$asset"
curl -fsSL "$base/$asset" -o "$tmp/$asset" || err "下载失败（该版本可能没有 $asset 产物）"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" || err "下载 SHA256SUMS 失败"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmp" && grep " $asset\$" SHA256SUMS | sha256sum -c -) || err "校验和不匹配"
fi
install -m 755 "$tmp/$asset" "$dir/$BIN"

printf '已安装 %s %s -> %s/%s\n' "$BIN" "$version" "$dir" "$BIN"
case ":$PATH:" in
  *":$dir:"*) printf '现在可以直接使用：git pomona --help\n' ;;
  *) printf '注意：%s 不在 PATH 中，请加入后使用。\n' "$dir" ;;
esac
