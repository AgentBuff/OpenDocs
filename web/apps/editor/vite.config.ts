import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const API_TARGET = process.env.OO_API ?? "http://127.0.0.1:8787";

export default defineConfig({
  plugins: [react()],
  server: {
    // 固定绑定本地 IPv4，避免 Vite 自动落到 IPv6 或占用其它服务的端口。
    host: "127.0.0.1",
    port: 5174,
    // 走代理而不是直连后端，前端代码里就不必区分开发和生产的 API 地址。
    proxy: {
      "/api": { target: API_TARGET, changeOrigin: true },
    },
  },
  worker: { format: "es" },
  build: { target: "es2022" },
});
