FROM node:24-bookworm-slim

WORKDIR /app
COPY mcp/package.json ./package.json
RUN npm install --omit=dev --ignore-scripts --no-audit --no-fund
COPY mcp/src ./src

ENV NODE_ENV=production
EXPOSE 8787
USER node
CMD ["node", "src/index.mjs"]
