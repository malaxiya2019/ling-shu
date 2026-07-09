# 部署指南

## Docker

```bash
docker pull ghcr.io/malaxiya2019/ling-shu:latest
docker run -d --name lingshu -p 8080:8080 --env-file .env ghcr.io/malaxiya2019/ling-shu:latest
```

## Termux (Android)

```bash
pkg install rust protobuf openssl
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh --china
```

## 裸机 Linux

```bash
./start.sh
# 或使用 Release 包
tar xzf lingshu-x86_64-unknown-linux-gnu.tar.gz
cd lingshu
./lingshu
```
