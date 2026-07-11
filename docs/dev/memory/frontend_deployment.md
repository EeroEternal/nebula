# Nebula 前端部署信息

Nebula 前端运行在远程服务器 10.21.11.92（用户名 ai），路径 `/home/ai/github/nebula/frontend`。

## 同步与部署流程

本地修改代码后需要 rsync 同步到远程（排除 node_modules 和 .git），然后在远程重启 vite dev server。

## 远程环境

- **Node 路径**：~/local/node-v20.18.1-linux-x64/bin

## 启动命令

```bash
export PATH=~/local/node-v20.18.1-linux-x64/bin:$PATH && cd ~/github/nebula/frontend && npx vite --host 0.0.0.0 --port 5173
```

## 技术栈

- Tailwind CSS v4 + @tailwindcss/postcss
- 在 v4 中，tailwind.config.ts 不会被读取
- 颜色必须通过 `@theme { --color-xxx: ... }` 在 index.css 中注册
- CSS 变量在 `@layer base :root {}` 中定义，在 `@theme` 块中映射
- postcss.config.js 使用 `@tailwindcss/postcss` 插件
