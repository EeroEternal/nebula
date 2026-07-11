# Nebula 开发环境设置指南

本文档介绍如何设置 Nebula 项目的开发环境所需的外部依赖。

## 快速克隆（推荐）

为了加快克隆速度，建议使用浅克隆（shallow clone）：

```bash
# 只克隆最近的提交历史
git clone --depth 1 https://github.com/lipish/nebula.git
# 或者克隆特定分支
git clone --depth 1 --branch main https://github.com/lipish/nebula.git
```

这将显著减少克隆时间和磁盘空间占用。

## 依赖项

### 1. etcd

Nebula 使用 etcd 作为元数据存储。

**下载和安装：**

```bash
# 创建 bin 目录（如果不存在）并确保在 PATH 中
mkdir -p ~/bin
echo 'export PATH="$HOME/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# 下载 etcd (根据您的操作系统选择合适的版本)
ETCD_VER=v3.5.12
DOWNLOAD_URL=https://github.com/etcd-io/etcd/releases/download

# Linux
curl -L ${DOWNLOAD_URL}/${ETCD_VER}/etcd-${ETCD_VER}-linux-amd64.tar.gz -o /tmp/etcd-${ETCD_VER}-linux-amd64.tar.gz
tar xzvf /tmp/etcd-${ETCD_VER}-linux-amd64.tar.gz -C /tmp
cp /tmp/etcd-${ETCD_VER}-linux-amd64/etcd* ~/bin/

# macOS
curl -L ${DOWNLOAD_URL}/${ETCD_VER}/etcd-${ETCD_VER}-darwin-amd64.tar.gz -o /tmp/etcd-${ETCD_VER}-darwin-amd64.tar.gz
tar xzvf /tmp/etcd-${ETCD_VER}-darwin-amd64.tar.gz -C /tmp
cp /tmp/etcd-${ETCD_VER}-darwin-amd64/etcd* ~/bin/
```

**验证安装：**

```bash
etcd --version
```

### 2. protoc (Protocol Buffers Compiler)

如果项目使用 Protocol Buffers，您需要安装 protoc。

**下载和安装：**

```bash
# 下载 protoc (根据您的操作系统选择合适的版本)
PROTOC_VER=25.1
DOWNLOAD_URL=https://github.com/protocolbuffers/protobuf/releases/download

# Linux
curl -LO ${DOWNLOAD_URL}/v${PROTOC_VER}/protoc-${PROTOC_VER}-linux-x86_64.zip
unzip protoc-${PROTOC_VER}-linux-x86_64.zip -d $HOME/.local
rm protoc-${PROTOC_VER}-linux-x86_64.zip

# macOS
curl -LO ${DOWNLOAD_URL}/v${PROTOC_VER}/protoc-${PROTOC_VER}-osx-x86_64.zip
unzip protoc-${PROTOC_VER}-osx-x86_64.zip -d $HOME/.local
rm protoc-${PROTOC_VER}-osx-x86_64.zip
```

**验证安装：**

```bash
protoc --version
```

## 构建项目

安装完依赖后，您可以构建项目：

```bash
cargo build --release
```

## 运行服务

使用提供的脚本启动所有服务：

```bash
# 启动所有服务
./bin/nebula-up.sh

# 停止所有服务
./bin/nebula-down.sh
```

## 注意事项

- 这些二进制文件不再包含在 Git 仓库中，以减小仓库大小并加快克隆速度
- 请从官方源下载最新稳定版本
- 确保将二进制文件放在 PATH 环境变量包含的目录中
