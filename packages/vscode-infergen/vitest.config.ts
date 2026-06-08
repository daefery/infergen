import { defineConfig } from 'vitest/config';
import * as path from 'path';

export default defineConfig({
  test: {
    environment: 'node',
  },
  resolve: {
    alias: {
      vscode: path.resolve(__dirname, 'src/test/__mocks__/vscode.ts'),
    },
  },
});
