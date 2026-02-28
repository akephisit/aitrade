# ── Frontend Dockerfile (Multi-stage) ─────────────────────────────────────────
FROM node:22-slim AS builder

WORKDIR /app
COPY package*.json ./
RUN npm ci

COPY . .
RUN npm run build

# Serve with a lightweight static server
FROM node:22-slim AS runtime
WORKDIR /app
COPY --from=builder /app/build ./build
COPY --from=builder /app/package.json ./

RUN npm install -g @sveltejs/adapter-node 2>/dev/null || true

EXPOSE 3001
ENV PORT=3001
ENV HOST=0.0.0.0
# หมายเหตุ: ต้องใช้ adapter-node หรือ adapter-static
# ดู frontend/svelte.config.js สำหรับ adapter ที่ใช้
CMD ["node", "build"]
