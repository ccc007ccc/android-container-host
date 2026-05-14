import path from 'node:path';
import { defineConfig } from 'vite';

const target = process.env.ACHOST_WEBUI_TARGET === 'lxc' ? 'lxc' : 'docker';
const title = target === 'lxc' ? 'LXC' : 'Docker';

export default defineConfig({
  base: './',
  resolve: {
    alias: {
      '/src/webui-entry.ts': path.resolve(__dirname, `src/${target}/main.ts`),
    },
  },
  plugins: [
    {
      name: 'achost-webui-target',
      transformIndexHtml(html) {
        return html.replace(/__ACHOST_WEBUI_TITLE__/g, title);
      },
    },
  ],
  build: {
    outDir: `dist/${target}`,
    emptyOutDir: true,
  },
});
