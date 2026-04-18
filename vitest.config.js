import { defineConfig } from 'vite';
import solidPlugin from 'vite-plugin-solid';

export default defineConfig({
  plugins: [solidPlugin()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['node_modules/@testing-library/jest-dom/vitest'],
    deps: {
      optimizer: {
        web: {
          include: ['@solidjs/testing-library']
        }
      }
    }
  },
  resolve: {
    conditions: ['development', 'browser'],
  }
});
